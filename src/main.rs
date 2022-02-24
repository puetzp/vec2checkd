mod config;
mod error;
mod helpers;
mod icinga;
mod types;
mod util;

use crate::icinga::*;
use crate::types::Mapping;
use crate::util::*;
use anyhow::Context;
use gumdrop::Options;
use log::{debug, error, info};
use prometheus_http_query::Client as PromClient;
use std::fs::File;
use std::io::Read;
use std::str::FromStr;
use std::time::Instant;

const DEFAULT_CONFIG_PATH: &str = "/etc/vec2checkd/config.yaml";
const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Options)]
struct AppOptions {
    #[options(help = "print help message", short = "h")]
    help: bool,

    #[options(help = "print version", short = "v")]
    version: bool,

    #[options(
        help = "load configuration file from a path other than the default",
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
        .with_context(|| "failed to parse mappings from configuration")?;

    if mappings.is_empty() {
        info!("No mappings configured. Shutdown.");
        std::process::exit(0);
    }

    let prom_client = {
        info!("Read Prometheus section from configuration");
        let c = config::parse_prom_section(&config)
            .with_context(|| "failed to parse Prometheus section from configuration")?;
        info!("Initialize Prometheus API client");
        match PromClient::from_str(&c.host)
            .with_context(|| "failed to initialize Prometheus API client")
        {
            Ok(clt) => clt,
            Err(e) => {
                error!("{:?}", e);
                std::process::exit(1)
            }
        }
    };

    let icinga_client = {
        info!("Read Icinga section from configuration");
        let c = config::parse_icinga_section(&config)
            .with_context(|| "failed to parse Icinga section from configuration")?;
        info!("Initialize Icinga API client");
        match IcingaClient::new(&c).with_context(|| "failed to initialize Icinga API client") {
            Ok(clt) => clt,
            Err(e) => {
                error!("{:?}", e);
                std::process::exit(1)
            }
        }
    };

    info!("Execute every check once regardless of the configured intervals and then enter the periodic check loop");
    let mut initial_check = true;
    loop {
        for mapping in mappings
            .iter_mut()
            .filter(|mapping| compute_delta(mapping).as_secs() <= 1 || initial_check)
        {
            let task_start = Instant::now();

            debug!(
                "{}: update last check time, set to {:?}",
                mapping.name, task_start
            );

            mapping.last_apply = task_start;

            match execute_task(prom_client.clone(), icinga_client.clone(), mapping.clone()).await {
                Ok(Ok(())) => {
                    debug!(
                        "'{}': check finished in {} millisecond(s)",
                        mapping.name,
                        task_start.elapsed().as_millis()
                    );
                    debug!(
                        "'{}': next check in ~{} second(s)",
                        mapping.name,
                        compute_delta(mapping).as_secs()
                    );
                }
                Ok(Err(err)) => {
                    error!("'{}': failed to finish check: {:?}", mapping.name, err);
                    debug!(
                        "'{}': retry check in ~{} second(s)",
                        mapping.name,
                        compute_delta(mapping).as_secs()
                    );
                }
                Err(err) => {
                    error!("'{}': failed to finish check: {:?}", mapping.name, err);
                    debug!(
                        "'{}': retry check in ~{} second(s)",
                        mapping.name,
                        compute_delta(mapping).as_secs()
                    );
                }
            }
        }
        initial_check = false;
        let sleep_secs = mappings.iter().map(compute_delta).min().unwrap();
        std::thread::sleep(sleep_secs);
    }
}
