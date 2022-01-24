mod config;
mod error;
mod icinga;
mod types;
mod util;

use crate::icinga::IcingaClient;
use crate::types::Mapping;
use crate::util::*;
use anyhow::anyhow;
use anyhow::Context;
use gumdrop::Options;
use log::{debug, error, info};
use prometheus_http_query::{Client, InstantVector};
use std::fs::File;
use std::io::Read;
use std::str::FromStr;
use std::time::Instant;

type TaskResult = Result<Result<(), anyhow::Error>, tokio::task::JoinError>;

const DEFAULT_CONFIG_PATH: &str = "/etc/vec2checkd/config.yaml";
const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Options)]
struct AppOptions {
    #[options(help = "print help message", short = "h")]
    help: bool,

    #[options(help = "print version", short = "v")]
    version: bool,

    #[options(
        help = "load configuration file from a path other than the default (/etc/vec2checkd/config.yaml)",
        short = "c"
    )]
    config: Option<String>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), anyhow::Error> {
    let opts = AppOptions::parse_args_default_or_exit();

    if opts.version {
        println!("v{}", VERSION);
        std::process::exit(0);
    }

    env_logger::init();

    let config_path = opts.config.unwrap_or_else(|| {
        info!(
            "No custom config file path provided, falling back to default location: {}",
            DEFAULT_CONFIG_PATH
        );
        DEFAULT_CONFIG_PATH.to_string()
    });

    let config = {
        info!("Parse configuration from '{}'", config_path);
        let mut file = File::open(&config_path)
            .with_context(|| format!("failed to read configuration file '{}'", config_path))?;

        let mut raw_conf = String::new();
        file.read_to_string(&mut raw_conf)?;

        config::parse_yaml(&raw_conf).with_context(|| "failed to parse configuration file")?
    };

    info!("Read mappings between PromQL and Icinga check results from configuration");
    let mut mappings: Vec<Mapping> = config::parse_mappings(config.clone())
        .with_context(|| "failed configuration to parse mappings from configuration")?;

    if mappings.is_empty() {
        info!("No mappings configured. Shutdown.");
        std::process::exit(0);
    }

    let prom_client = {
        info!("Read Prometheus section from configuration");
        match config::parse_prom_section(&config)
            .with_context(|| "failed to parse Prometheus section from configuration")?
        {
            Some(c) => {
                info!(
                    "Initialize Prometheus API client using base URL '{}://{}:{}'",
                    c.scheme, c.host, c.port
                );
                Client::new(c.scheme, &c.host, c.port)
            }
            None => {
                info!("Initialize Prometheus API client using base URL 'http://127.0.0.1:9090'");
                Client::default()
            }
        }
    };

    let icinga_client = {
        info!("Read Icinga section from configuration");
        let c = config::parse_icinga_section(&config)
            .with_context(|| "failed to parse Icinga section from configuration")?;
        info!("Initialize Icinga API client");
        IcingaClient::new(&c).with_context(|| "failed to initialize Icinga API client")?
    };

    info!("Enter the main check loop");
    loop {
        let sleep_secs = mappings
            .iter()
            .map(|mapping| compute_delta(&mapping))
            .min()
            .unwrap();

        std::thread::sleep(sleep_secs);

        for mapping in mappings
            .iter_mut()
            .filter(|mapping| compute_delta(&mapping).as_secs() <= 1)
        {
            let task_start = Instant::now();

            debug!(
                "{}: update last application clock time, set to {:?}",
                mapping.name, task_start
            );

            mapping.last_apply = task_start;

            let inner_prom_client = prom_client.clone();
            let prom_query = mapping.query.to_string();

            let inner_icinga_client = icinga_client.clone();

            let inner_mapping = mapping.clone();

            let join_handle: TaskResult = tokio::spawn(async move {
                let exec_start = util::get_unix_timestamp().with_context(|| {
                    "failed to retrieve UNIX timestamp to measure event execution"
                })?;

                debug!(
                    "{}: start processing mapping at {}",
                    inner_mapping.name, exec_start
                );

                debug!(
                    "{}: execute PromQL query '{}'",
                    inner_mapping.name, prom_query
                );

                let vector = InstantVector(prom_query);

                let abstract_vector = inner_prom_client
                    .query(vector, None, None)
                    .await
                    .with_context(|| "failed to execute PromQL query")?;

                let instant_vector = abstract_vector
                    .as_instant()
                    .ok_or(anyhow!(
                        "failed to parse PromQL query result as instant vector"
                    ))?
                    .get(0)
                    .ok_or(anyhow!("the PromQL result is empty"))?;

                let value = f64::from_str(instant_vector.sample().value())
                    .with_context(|| "failed to convert value of PromQL query result to float")?;

                let exit_status = match &inner_mapping.thresholds {
                    Some(thresholds) => icinga::determine_exit_status(&thresholds, value),
                    None => 0,
                };

                let exec_end = util::get_unix_timestamp().with_context(|| {
                    "failed to retrieve UNIX timestamp to measure event execution"
                })?;

                debug!(
                    "{}: stop measuring processing of mapping at {}",
                    inner_mapping.name, exec_end
                );

                inner_icinga_client
                    .send(&inner_mapping, value, exit_status, exec_start, exec_end)
                    .await
                    .with_context(|| "failed to send passive check result to Icinga")?;

                debug!(
                    "{}: passive check result was successfully send to Icinga",
                    inner_mapping.name
                );

                Ok(())
            })
            .await;

            match join_handle {
                Ok(Ok(())) => info!(
                    "{}: task finished in {} milliseconds",
                    mapping.name,
                    task_start.elapsed().as_millis()
                ),
                Ok(Err(e)) => error!(
                    "{}: failed to finish task: {}",
                    mapping.name,
                    e.root_cause()
                ),
                Err(e) => error!("{}: failed to finish task: {}", mapping.name, e),
            }
        }
    }
}
