mod config;
mod error;
mod types;

use crate::types::Mapping;
use anyhow::Context;
use log::info;
use prometheus_http_query::{Client, InstantVector};
use std::fs::File;
use std::io::Read;
use std::time::{Duration, Instant};

const DEFAULT_CONFIG_PATH: &str = "/etc/vec2checkd/config.yaml";

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), anyhow::Error> {
    env_logger::init();

    info!("Parsing configuration from '{}'", DEFAULT_CONFIG_PATH);
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

    let prom_client = Client::default();

    loop {
        let sleep_secs = {
            let now = Instant::now();
            mappings
                .iter()
                .map(|mapping| {
                    let elapsed = now.saturating_duration_since(mapping.last_apply);
                    mapping.interval.saturating_sub(elapsed)
                })
                .min()
                .unwrap()
        };

        println!("Sleeping for: {:?}", sleep_secs);
        std::thread::sleep(sleep_secs);

        let now = Instant::now();
        mappings
            .iter_mut()
            .filter(|mapping| {
                let delta = {
                    let elapsed = now.saturating_duration_since(mapping.last_apply);
                    mapping.interval.saturating_sub(elapsed)
                };
                delta.as_secs() <= 1
            })
            .for_each(|mapping| {
                mapping.last_apply = Instant::now();
                println!("Virtual run of: {}", mapping.name);
            });

        /*
        for mapping in mappings.iter() {
            let client = prom_client.clone();
            let query = mapping.query.to_string();
            let handle = tokio::spawn(async move {
                let vector = InstantVector(query);
                return client.query(vector, None, None).await.unwrap();
            });
            match handle.await {
                Ok(r) => println!("{:?}", r),
                Err(e) => println!("{:?}", e),
            };
        }
         */
    }
    Ok(())
}
