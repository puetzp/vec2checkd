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
use log::{debug, error, info, warn};
use prometheus_http_query::{Client, InstantVector};
use std::fs::File;
use std::io::Read;
use std::str::FromStr;
use std::time::Instant;

const DEFAULT_CONFIG_PATH: &str = "/etc/vec2checkd/config.yaml";

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), anyhow::Error> {
    env_logger::init();

    info!("Parse configuration from '{}'", DEFAULT_CONFIG_PATH);
    let mut file = File::open(DEFAULT_CONFIG_PATH).with_context(|| {
        format!(
            "failed to read configuration file '{}'",
            DEFAULT_CONFIG_PATH
        )
    })?;

    let mut raw_conf = String::new();
    file.read_to_string(&mut raw_conf)?;

    let config =
        config::parse_yaml(&raw_conf).with_context(|| "failed to parse configuration file")?;

    info!("Read mappings section from configuration");
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
            let now = Instant::now();

            debug!(
                "{}: update last application clock time, set to {:?}",
                mapping.name, now
            );

            mapping.last_apply = now;

            let inner_prom_client = prom_client.clone();
            let prom_query = mapping.query.to_string();

            let inner_icinga_client = icinga_client.clone();

            let inner_mapping = mapping.clone();

            let join_handle = tokio::spawn(async move {
                let exec_start = util::get_unix_timestamp()
                    .with_context(|| "failed to retrieve UNIX timestamp to measure event execution")
                    .unwrap();

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
                    .with_context(|| "failed to execute PromQL query")
                    .unwrap();

                let instant_vector = abstract_vector
                    .as_instant()
                    .ok_or(anyhow!(
                        "failed to parse PromQL query result as instant vector"
                    ))
                    .unwrap()
                    .get(0)
                    .ok_or(anyhow!("the PromQL result is empty"))
                    .unwrap();

                let value = f64::from_str(instant_vector.sample().value())
                    .with_context(|| "failed to convert value of PromQL query result to float")
                    .unwrap();

                let exit_status = match &inner_mapping.thresholds {
                    Some(thresholds) => icinga::determine_exit_status(&thresholds, value),
                    None => 0,
                };

                let exec_end = util::get_unix_timestamp()
                    .with_context(|| "failed to retrieve UNIX timestamp to measure event execution")
                    .unwrap();

                debug!(
                    "{}: stop measuring processing of mapping at {}",
                    inner_mapping.name, exec_end
                );

                inner_icinga_client
                    .send(&inner_mapping, value, exit_status, exec_start, exec_end)
                    .await
                    .with_context(|| "failed to send passive check result to Icinga")
                    .unwrap();

                debug!(
                    "{}: passive check result was successfully send to Icinga",
                    inner_mapping.name
                );
            })
            .await;

            match join_handle {
                Ok(result) => result,
                Err(e) => {
                    error!("{}: failed to finish task: {}", mapping.name, e);
                    continue;
                }
            };
        }
    }
    Ok(())
}
