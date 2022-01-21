mod config;
mod error;
mod icinga;
mod types;
mod util;

use crate::icinga::IcingaClient;
use crate::types::Mapping;
use crate::util::*;
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
    let mut mappings: Vec<Mapping> = config::parse_mappings(&config)
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
        let sleep_secs = {
            mappings
                .iter()
                .map(|mapping| compute_delta(&mapping))
                .min()
                .unwrap()
        };

        std::thread::sleep(sleep_secs);

        for mapping in mappings
            .iter_mut()
            .filter(|mapping| compute_delta(&mapping).as_secs() <= 1)
        {
            let exec_start = match util::get_unix_timestamp() {
                Ok(start) => {
                    debug!("{}: Start processing mapping at {}", mapping.name, start);
                    start
                }
                Err(e) => {
                    error!("{}: Skip mapping due to error: {}", mapping.name, e);
                    continue;
                }
            };

            debug!("Process mapping '{}'", mapping.name);
            let now = Instant::now();
            debug!(
                "{}: update last application clock time, set to {:?}",
                mapping.name, now
            );
            mapping.last_apply = now;

            let client = prom_client.clone();
            let query = mapping.query.to_string();

            debug!("{}: execute PromQL query '{}'", mapping.name, query);

            let join_handle = tokio::spawn(async move {
                let vector = InstantVector(query);
                return client.query(vector, None, None).await;
            })
            .await;

            let query_result = match join_handle {
                Ok(result) => result,
                Err(e) => {
                    error!("{}: failed to finish task: {}", mapping.name, e);
                    continue;
                }
            };

            let abstract_vector = match query_result {
                Ok(vector) => vector,
                Err(e) => {
                    error!("{}: failed to execute PromQL query: {}", mapping.name, e);
                    continue;
                }
            };

            let instant_vector = match abstract_vector.as_instant() {
                Some(instant) => match instant.get(0) {
                    Some(first) => first,
                    None => {
                        warn!("{}: the PromQL query result is empty", mapping.name);
                        continue;
                    }
                },
                None => {
                    error!(
                        "{}: failed to parse PromQL query result as instant vector",
                        mapping.name
                    );
                    continue;
                }
            };

            let value = match f64::from_str(instant_vector.sample().value()) {
                Ok(v) => v,
                Err(e) => {
                    error!(
                        "{}: failed to convert value of PromQL query result to float: {}",
                        mapping.name, e
                    );
                    continue;
                }
            };

            let exit_status = match &mapping.thresholds {
                Some(thresholds) => icinga::determine_exit_status(&thresholds, value),
                None => 0,
            };

            let exec_end = match util::get_unix_timestamp() {
                Ok(end) => {
                    debug!(
                        "{}: Stop measuring processing of mapping at {}",
                        mapping.name, end
                    );
                    end
                }
                Err(e) => {
                    error!(
                        "{}: Further processing skipped due to error: {}",
                        mapping.name, e
                    );
                    continue;
                }
            };

            match icinga_client
                .send(&mapping, value, exit_status, exec_start, exec_end)
                .await
            {
                Ok(_) => debug!(
                    "{}: passive check result was successfully send to Icinga",
                    mapping.name
                ),
                Err(e) => {
                    error!(
                        "{}: failed to send passive check result to Icinga: {}",
                        mapping.name, e
                    );
                    continue;
                }
            }
        }
    }
    Ok(())
}
