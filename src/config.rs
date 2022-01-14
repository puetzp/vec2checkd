use crate::error::*;
use crate::types::{Mapping, ThresholdPair};
use anyhow::{anyhow, Context};
use nagios_range::NagiosRange;
use yaml_rust::yaml::Yaml;
use yaml_rust::YamlLoader;

fn parse_mapping<'a>(mapping: (&'a Yaml, &'a Yaml)) -> Result<Mapping<'a>, anyhow::Error> {
    let name = mapping.0.as_str().ok_or(ParseFieldError {
        field: format!("mappings.$name"),
        kind: "string",
    })?;

    let items = mapping.1.as_hash().ok_or(ParseFieldError {
        field: format!("mappings.{}", name),
        kind: "hash",
    })?;

    let query = items
        .get(&Yaml::from_str("query"))
        .ok_or(MissingFieldError {
            field: format!("mappings.{}.query", name),
        })?
        .as_str()
        .ok_or(ParseFieldError {
            field: format!("mappings.{}.query", name),
            kind: "string",
        })?;

    let host = items
        .get(&Yaml::from_str("host"))
        .ok_or(MissingFieldError {
            field: format!("mappings.{}.host", name),
        })?
        .as_str()
        .ok_or(ParseFieldError {
            field: format!("mappings.{}.host", name),
            kind: "string",
        })?;

    let service = items
        .get(&Yaml::from_str("service"))
        .ok_or(MissingFieldError {
            field: format!("mappings.{}.service", name),
        })?
        .as_str()
        .ok_or(ParseFieldError {
            field: format!("mappings.{}.service", name),
            kind: "string",
        })?;

    let thresholds = match items.get(&Yaml::from_str("thresholds")) {
        Some(t) => {
            let t_hash = t.as_hash().ok_or(ParseFieldError {
                field: format!("mappings.{}.thresholds", name),
                kind: "hash",
            })?;

            if t_hash.is_empty() {
                None
            } else {
                Some(t_hash)
            }
        }
        None => None,
    };

    let threshold_pair = match thresholds {
        Some(t) => {
            let warning = match t.get(&Yaml::from_str("warning")) {
                Some(w) => {
                    let w_raw = w.as_str().ok_or(ParseFieldError {
                        field: format!("mappings.{}.thresholds.warning", name),
                        kind: "string",
                    })?;
                    Some(NagiosRange::from(w_raw)?)
                }
                None => None,
            };

            let critical = match t.get(&Yaml::from_str("critical")) {
                Some(c) => {
                    let c_raw = c.as_str().ok_or(ParseFieldError {
                        field: format!("mappings.{}.thresholds.critical", name),
                        kind: "string",
                    })?;
                    Some(NagiosRange::from(c_raw)?)
                }
                None => None,
            };

            Some(ThresholdPair { warning, critical })
        }
        None => None,
    };

    Ok(Mapping {
        name,
        query,
        host,
        service,
        thresholds: threshold_pair,
    })
}

pub(crate) fn parse_mappings<'a>(
    config: &'a [Yaml],
) -> Result<Option<Vec<Mapping<'a>>>, anyhow::Error> {
    let mut mappings: Vec<Mapping> = vec![];

    match config[0]
        .as_hash()
        .ok_or(anyhow!("failed to parse configuration as hash"))?
        .get(&Yaml::from_str("mappings"))
    {
        Some(m_raw) => {
            let mapping_hash = m_raw.as_hash().ok_or(ParseFieldError {
                field: String::from("mappings"),
                kind: "hash",
            })?;

            for raw_mapping in mapping_hash {
                let parsed = parse_mapping(raw_mapping)?;
                mappings.push(parsed);
            }

            Ok(Some(mappings))
        }
        None => Ok(None),
    }
}

pub(crate) fn parse_yaml(source: &str) -> Result<Vec<Yaml>, yaml_rust::scanner::ScanError> {
    yaml_rust::yaml::YamlLoader::load_from_str(source)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;

    #[test]
    fn test_parse_mappings() {
        let s = "
---
mappings:
  cpu_idle_percentage:
    query: 'sum(node_cpu_seconds_total{mode=\"idle\"}) / sum(node_cpu_seconds_total)'
    host: 'Kubernetes Test'
    service: 'CPU idle percentage'
    interval: '1m'
";
        let result = vec![Mapping {
            name: String::from(),
        }];
    }
}
