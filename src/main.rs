mod config;
mod error;
mod types;

use crate::types::Mapping;
use anyhow::Context;
use log::{debug, error, info};
use prometheus_http_query::{Client, InstantVector};
use std::fs::File;
use std::io::Read;
use std::time::{Duration, Instant};

const DEFAULT_CONFIG_PATH: &str = "/etc/vec2checkd/config.yaml";

fn compute_delta(mapping: &Mapping) -> Duration {
    mapping
        .interval
        .saturating_sub(mapping.last_apply.elapsed())
}

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

    let config = config::parse_yaml(&raw_conf).with_context(|| "failed to parse configuration")?;

    let mut mappings: Vec<Mapping> = config::parse_mappings(&config)
        .with_context(|| "failed configuration to parse mappings from configuration")?;

    if mappings.is_empty() {
        info!("No mappings configured. Exiting.");
        std::process::exit(0);
    }

    info!("Initialize Prometheus client");
    let prom_client = Client::default();

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
            info!("Process mapping '{}'", mapping.name);
            let now = Instant::now();
            debug!(
                "{}: update last application clock time, set to {:?}",
                mapping.name, now
            );
            mapping.last_apply = now;
            let client = prom_client.clone();
            let query = mapping.query.to_string();
            debug!("{}: execute PromQL query '{}'", mapping.name, query);
            let query_result = tokio::spawn(async move {
                let vector = InstantVector(query);
                return client.query(vector, None, None).await;
            })
            .await?;

            let vector = match query_result {
                Ok(v) => v,
                Err(e) => {
                    error!("{}: failed to execute PromQL query: {}", mapping.name, e);
                    continue;
                }
            };
        }
    }
    Ok(())
}
