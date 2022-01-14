mod config;
mod error;
mod types;

use crate::types::Mapping;
use anyhow::Context;
use log::info;

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
    let mappings: Option<Vec<Mapping>> = config::parse_mappings(s)
        .with_context(|| "An error occurred while parsing the configuration")?;

    println!("{:?}", mappings);
    Ok(())
}
