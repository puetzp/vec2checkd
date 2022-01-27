use crate::error::*;
use crate::types::*;
use anyhow::{anyhow, bail};
use nagios_range::NagiosRange;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use yaml_rust::yaml::{Hash, Yaml};

/// This function replaces all placeholders in the custom plugin string
/// that are already known after parsing a mapping and do not change
/// during runtime.
fn preformat_plugin_output(mapping: &mut Mapping) -> Result<(), anyhow::Error> {
    if let Some(ref mut plugin_output) = mapping.plugin_output {
        // Copy the templated plugin output in order to gradually replace
        // placeholders with values.
        //        let mut plugin_output = template.to_string();

        // Every placeholder whose value is known at this point is replaced.
        // Placeholders whose values depend on the result of the PromQL query
        // will be replaced later on.
        *plugin_output = plugin_output.replace("$name", &mapping.name);
        *plugin_output = plugin_output.replace("$query", &mapping.query);
        *plugin_output =
            plugin_output.replace("$interval", &mapping.interval.as_secs().to_string());
        *plugin_output = plugin_output.replace("$host", &mapping.host);

        let placeholder = "$service";
        if plugin_output.contains(placeholder) {
            if let Some(replacement) = &mapping.service {
                *plugin_output = plugin_output.replace(placeholder, replacement);
            } else {
                bail!("'{}': cannot replace plugin output placeholder '{}' as no service name was configured", placeholder, mapping.name);
            }
        }

        let placeholder = "$thresholds.warning";
        if plugin_output.contains(placeholder) {
            if let Some(s) = &mapping.thresholds.warning {
                *plugin_output = plugin_output.replace(placeholder, &s.to_string());
            } else {
                bail!("'{}': cannot replace plugin output placeholder '{}' as no warning threshold was configured", placeholder, mapping.name);
            }
        }

        let placeholder = "$thresholds.critical";
        if plugin_output.contains(placeholder) {
            if let Some(s) = &mapping.thresholds.critical {
                *plugin_output = plugin_output.replace(placeholder, &s.to_string());
            } else {
                bail!("'{}': cannot replace plugin output placeholder '{}' as no critical threshold was configured", placeholder, mapping.name);
            }
        }

        Ok(())
    } else {
        Ok(())
    }
}

fn parse_mapping(mapping: (&Yaml, &Yaml)) -> Result<Mapping, anyhow::Error> {
    let name = mapping
        .0
        .as_str()
        .ok_or(ParseFieldError {
            field: format!("mappings.$name"),
            kind: "string",
        })?
        .to_string();

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
        })?
        .to_string();

    let host = items
        .get(&Yaml::from_str("host"))
        .ok_or(MissingFieldError {
            field: format!("mappings.{}.host", name),
        })?
        .as_str()
        .ok_or(ParseFieldError {
            field: format!("mappings.{}.host", name),
            kind: "string",
        })?
        .to_string();

    let service = match items.get(&Yaml::from_str("service")) {
        Some(s) => Some(
            s.as_str()
                .ok_or(ParseFieldError {
                    field: format!("mappings.{}.service", name),
                    kind: "string",
                })?
                .to_string(),
        ),
        None => None,
    };

    let plugin_output = match items.get(&Yaml::from_str("plugin_output")) {
        Some(p) => Some(
            p.as_str()
                .ok_or(ParseFieldError {
                    field: format!("mappings.{}.plugin_output", name),
                    kind: "string",
                })?
                .to_string(),
        ),
        None => None,
    };

    let thresholds = {
        match items.get(&Yaml::from_str("thresholds")) {
            Some(t) => {
                let t_hash = t.as_hash().ok_or(ParseFieldError {
                    field: format!("mappings.{}.thresholds", name),
                    kind: "hash",
                })?;

                if t_hash.is_empty() {
                    ThresholdPair::default()
                } else {
                    let warning = match t_hash.get(&Yaml::from_str("warning")) {
                        Some(w) => {
                            let w_raw = w.as_str().ok_or(ParseFieldError {
                                field: format!("mappings.{}.thresholds.warning", name),
                                kind: "string",
                            })?;
                            Some(NagiosRange::from(w_raw)?)
                        }
                        None => None,
                    };

                    let critical = match t_hash.get(&Yaml::from_str("critical")) {
                        Some(c) => {
                            let c_raw = c.as_str().ok_or(ParseFieldError {
                                field: format!("mappings.{}.thresholds.critical", name),
                                kind: "string",
                            })?;
                            Some(NagiosRange::from(c_raw)?)
                        }
                        None => None,
                    };

                    ThresholdPair { warning, critical }
                }
            }
            None => ThresholdPair::default(),
        }
    };

    let interval: Duration = match items.get(&Yaml::from_str("interval")) {
        Some(i) => {
            let num = i.as_i64().ok_or(ParseFieldError {
                field: format!("mappings.{}.interval", name),
                kind: "number",
            })?;

            let conv = u16::try_from(num).map_err(|_| ParseFieldError {
                field: format!("mappings.{}.interval", name),
                kind: "number",
            })? as u64;

            let valid_range = 10..=3600;

            if !valid_range.contains(&conv) {
                return Err(anyhow!(
                    "mappings.{}.interval must be in the range {:?}, got {}",
                    name,
                    valid_range,
                    conv
                ));
            }

            Duration::from_secs(conv)
        }
        None => Duration::from_secs(60),
    };

    Ok(Mapping {
        name,
        query,
        host,
        service,
        interval,
        plugin_output,
        thresholds,
        last_apply: Instant::now(),
    })
}

pub(crate) fn parse_mappings(config: Hash) -> Result<Vec<Mapping>, anyhow::Error> {
    let mut mappings: Vec<Mapping> = vec![];

    match config.get(&Yaml::from_str("mappings")) {
        Some(m_raw) => {
            let mapping_hash = m_raw.as_hash().ok_or(ParseFieldError {
                field: String::from("mappings"),
                kind: "hash",
            })?;

            for raw_mapping in mapping_hash {
                let mut mapping = parse_mapping(raw_mapping)?;
                preformat_plugin_output(&mut mapping)?;
                mappings.push(mapping);
            }

            Ok(mappings)
        }
        None => Ok(vec![]),
    }
}

pub(crate) fn parse_prom_section(config: &Hash) -> Result<PromConfig, anyhow::Error> {
    let default_host = "http://localhost:9090";

    match config.get(&Yaml::from_str("prometheus")) {
        Some(section) => {
            let prometheus = section.as_hash().ok_or(ParseFieldError {
                field: String::from("prometheus"),
                kind: "hash",
            })?;

            let host = prometheus
                .get(&Yaml::from_str("host"))
                .unwrap_or(&Yaml::from_str(default_host))
                .as_str()
                .ok_or(ParseFieldError {
                    field: String::from("prometheus.host"),
                    kind: "string",
                })?
                .to_string();

            Ok(PromConfig { host })
        }
        None => Ok(PromConfig {
            host: default_host.to_string(),
        }),
    }
}

pub(crate) fn parse_icinga_section(config: &Hash) -> Result<IcingaConfig, anyhow::Error> {
    let section = config
        .get(&Yaml::from_str("icinga"))
        .ok_or(MissingFieldError {
            field: String::from("icinga"),
        })?
        .as_hash()
        .ok_or(ParseFieldError {
            field: String::from("icinga"),
            kind: "hash",
        })?;

    let host = section
        .get(&Yaml::from_str("host"))
        .unwrap_or(&Yaml::from_str("https://localhost:5665"))
        .as_str()
        .ok_or(ParseFieldError {
            field: String::from("icinga.host"),
            kind: "string",
        })?
        .to_string();

    let ca_cert = match section.get(&Yaml::from_str("ca_cert")) {
        Some(cert) => Some(
            cert.as_str()
                .ok_or(ParseFieldError {
                    field: String::from("icinga.ca_cert"),
                    kind: "string",
                })
                .map(|p| PathBuf::from(p))?,
        ),
        None => None,
    };

    let auth_hash = section
        .get(&Yaml::from_str("authentication"))
        .ok_or(MissingFieldError {
            field: String::from("icinga.authentication"),
        })?
        .as_hash()
        .ok_or(ParseFieldError {
            field: String::from("icinga.authentication"),
            kind: "hash",
        })?;

    let auth_method = auth_hash
        .get(&Yaml::from_str("method"))
        .ok_or(MissingFieldError {
            field: String::from("icinga.authentication.method"),
        })?
        .as_str()
        .ok_or(ParseFieldError {
            field: String::from("icinga.authentication.method"),
            kind: "string",
        })?;

    let authentication = match auth_method {
        "basic-auth" => {
            let username = auth_hash
                .get(&Yaml::from_str("username"))
                .ok_or(MissingFieldError {
                    field: String::from("icinga.authentication.username"),
                })?
                .as_str()
                .ok_or(ParseFieldError {
                    field: String::from("icinga.authentication.username"),
                    kind: "string",
                })?
                .to_string();

            let password = auth_hash
                .get(&Yaml::from_str("password"))
                .ok_or(MissingFieldError {
                    field: String::from("icinga.authentication.password"),
                })?
                .as_str()
                .ok_or(ParseFieldError {
                    field: String::from("icinga.authentication.password"),
                    kind: "string",
                })?
                .to_string();

            IcingaAuth::Basic(IcingaBasicAuth { username, password })
        }
        "x509" => {
            let client_cert = auth_hash
                .get(&Yaml::from_str("client_cert"))
                .ok_or(MissingFieldError {
                    field: String::from("icinga.authentication.client_cert"),
                })?
                .as_str()
                .ok_or(ParseFieldError {
                    field: String::from("icinga.authentication.client_cert"),
                    kind: "string",
                })
                .map(|p| PathBuf::from(p))?;

            let client_key = auth_hash
                .get(&Yaml::from_str("client_key"))
                .ok_or(MissingFieldError {
                    field: String::from("icinga.authentication.client_key"),
                })?
                .as_str()
                .ok_or(ParseFieldError {
                    field: String::from("icinga.authentication.client_key"),
                    kind: "string",
                })
                .map(|p| PathBuf::from(p))?;

            IcingaAuth::X509(IcingaX509Auth {
                client_cert,
                client_key,
            })
        }
        _ => {
            bail!(
                    "invalid value in 'icinga.authentication.method', must be either 'basic-auth' or 'x509'"
                )
        }
    };

    Ok(IcingaConfig {
        host,
        ca_cert,
        authentication,
    })
}

pub(crate) fn parse_yaml(source: &str) -> Result<Hash, anyhow::Error> {
    yaml_rust::yaml::YamlLoader::load_from_str(source)?[0]
        .clone()
        .into_hash()
        .ok_or(anyhow!("failed to parse configuration as hash"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Mapping, ThresholdPair};
    use nagios_range::NagiosRange;
    use std::time::{Duration, Instant};

    #[test]
    fn test_preformat_plugin_output_with_name() {
        let mut mapping = Mapping {
            name: "this check".to_string(),
            query: "up{random_label=\"random_value\"}".to_string(),
            thresholds: ThresholdPair::default(),
            host: "foo".to_string(),
            service: None,
            interval: Duration::from_secs(60),
            last_apply: Instant::now(),
            plugin_output: Some(String::from("Do not worry, $name is alright")),
        };
        preformat_plugin_output(&mut mapping).unwrap();
        assert_eq!(
            mapping.plugin_output.unwrap(),
            String::from("Do not worry, this check is alright")
        );
    }

    #[test]
    fn test_preformat_plugin_output_with_query() {
        let mut mapping = Mapping {
            name: "foobar".to_string(),
            query: "up{random_label=\"random_value\"}".to_string(),
            thresholds: ThresholdPair::default(),
            host: "foo".to_string(),
            service: None,
            interval: Duration::from_secs(60),
            last_apply: Instant::now(),
            plugin_output: Some(String::from("Query $query was successful")),
        };
        preformat_plugin_output(&mut mapping).unwrap();
        assert_eq!(
            mapping.plugin_output.unwrap(),
            String::from("Query up{random_label=\"random_value\"} was successful")
        );
    }

    #[test]
    fn test_preformat_plugin_output_with_name_and_interval() {
        let mut mapping = Mapping {
            name: "infallible".to_string(),
            query: "up{random_label=\"random_value\"}".to_string(),
            thresholds: ThresholdPair::default(),
            host: "foo".to_string(),
            service: None,
            interval: Duration::from_secs(60),
            last_apply: Instant::now(),
            plugin_output: Some(String::from(
                "Check '$name' is executed every $interval seconds",
            )),
        };
        preformat_plugin_output(&mut mapping).unwrap();
        assert_eq!(
            mapping.plugin_output.unwrap(),
            String::from("Check 'infallible' is executed every 60 seconds")
        );
    }

    #[test]
    fn test_preformat_plugin_output_with_threshold() {
        let mut mapping = Mapping {
            name: "foobar".to_string(),
            query: "up{random_label=\"random_value\"}".to_string(),
            thresholds: ThresholdPair {
                warning: None,
                critical: Some(NagiosRange::from("@10:20").unwrap()),
            },
            host: "foo".to_string(),
            service: None,
            interval: Duration::from_secs(60),
            last_apply: Instant::now(),
            plugin_output: Some(String::from(
                "Result value is $value (critical at: '$thresholds.critical')",
            )),
        };
        preformat_plugin_output(&mut mapping).unwrap();
        assert_eq!(
            mapping.plugin_output.unwrap(),
            String::from("Result value is $value (critical at: '@10:20')")
        );
    }
}
