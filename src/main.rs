mod config;
mod types;

use crate::types::Mapping;
use log::info;

fn main() {
    env_logger::init();
    let s = "
---
mappings:
  cpu_idle_percentage:
    query: 'sum(node_cpu_seconds_total{mode=\"idle\"}) / sum(node_cpu_seconds_total)'
    thresholds: {}
    host: 'Kubernetes Test'
    service: 'CPU idle percentage'
    interval: '1m'
";
    let mappings: Vec<Mapping> = match config::parse_mappings_from_config(s) {
        Ok(v) => v,
        Err(e) => panic!("An error occurred while parsing the configuration: {}", e),
    };
}
