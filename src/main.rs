use log::info;
use yaml_rust::yaml::Yaml;
use yaml_rust::YamlLoader;

struct NagiosRange(String);

struct Thresholds {
    warning: NagiosRange,
    critical: NagiosRange,
}

struct Mapping {
    query: String,
    thresholds: Thresholds,
    host: String,
    service: String,
}

fn parse_mapping(mapping: (&Yaml, &Yaml)) -> bool {
    true
}

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
    let docs = YamlLoader::load_from_str(s).unwrap();
    let doc = &docs[0];
    let mut mappings: Vec<Mapping> = vec![];

    match doc.as_hash().unwrap().get(&Yaml::from_str("mappings")) {
        Some(conf_mappings) => {
            let conf_mappings = match conf_mappings.as_hash() {
                Some(m) => m,
                None => panic!("'mappings' must be a hash"),
            };

            for map_item in conf_mappings {
                let parsed = parse_mapping(map_item);
                mappings.push(parsed);
            }
        }
        None => panic!("Missing mandatory configuration item"),
    }
}
