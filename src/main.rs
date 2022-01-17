mod config;
mod error;
mod types;

use crate::types::Mapping;
use anyhow::Context;
use log::info;
use std::fs::File;
use std::io::Read;

const DEFAULT_CONFIG_PATH: &str = "/etc/vec2checkd/config.yaml";

fn main() -> Result<(), anyhow::Error> {
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

    let mappings: Vec<Mapping> = config::parse_mappings(&config)
        .with_context(|| "failed configuration to parse mappings from configuration")?;

    if mappings.is_empty() {
        info!("No mappings configured. Exiting.");
        std::process::exit(0);
    }

    Ok(())
}
