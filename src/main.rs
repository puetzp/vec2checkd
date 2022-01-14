mod config;
mod error;
mod types;

use crate::types::Mapping;
use anyhow::Context;
use log::info;
use yaml_rust::yaml::YamlLoader;

fn main() -> Result<(), anyhow::Error> {
    env_logger::init();
    let s = "
---
mappings:
  cpu_idle_percentage:
    query: 'sum(node_cpu_seconds_total{mode=\"idle\"}) / sum(node_cpu_seconds_total)'
    host: 'Kubernetes Test'
    service: 'CPU idle percentage'
    interval: '1m'
";
    let config = config::parse_yaml(s).with_context(|| "failed to parse configuration")?;

    let mappings: Option<Vec<Mapping>> = config::parse_mappings(&config)
        .with_context(|| "failed configuration to parse mappings from configuration")?;

    if mappings.is_none() {
        info!("No mappings configured. Exiting.");
        std::process::exit(0);
    }
    println!("{:?}", mappings);
    Ok(())
}
