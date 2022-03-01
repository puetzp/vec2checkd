use crate::error::*;
use crate::types::*;
use anyhow::{anyhow, bail};
use log::debug;
use nagios_range::NagiosRange;
use std::env;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use yaml_rust::yaml::{Hash, Yaml};

/// Parses a single mapping from YAML configuration.
/// This YAML is expected to have the following format:
///
/// ```yaml
/// '<name>':
///   query: '<promql_query>'
///   host: '<host_object>'
///   service: '<host_object>'           # optional
///   interval: <check_interval>
///   thresholds:                        # optional
///     warning: '<nagios_range>'        # optional
///     critical: '<nagios_range>'       # optional
///   plugin_output: '<custom_template>' # optional
/// ```
fn parse_mapping(mapping: (&Yaml, &Yaml)) -> Result<Mapping, anyhow::Error> {
    let name = mapping
        .0
        .as_str()
        .ok_or(ParseFieldError {
            field: "mappings.$name".to_string(),
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

    let performance_data = {
        match items.get(&Yaml::from_str("performance_data")) {
            Some(t) => {
                let t_hash = t.as_hash().ok_or(ParseFieldError {
                    field: format!("mappings.{}.performance_data", name),
                    kind: "hash",
                })?;

                if t_hash.is_empty() {
                    PerformanceData::default()
                } else {
                    let enabled = match t_hash.get(&Yaml::from_str("enabled")) {
                        Some(val) => val.as_bool().ok_or(ParseFieldError {
                            field: format!("mappings.{}.performance_data.enabled", name),
                            kind: "boolean",
                        })?,
                        None => true,
                    };

                    let label = match t_hash.get(&Yaml::from_str("label")) {
                        Some(val) => {
                            let label = val
                                .as_str()
                                .ok_or(ParseFieldError {
                                    field: format!("mappings.{}.performance_data.label", name),
                                    kind: "string",
                                })?
                                .to_string();
                            Some(label)
                        }
                        None => None,
                    };

                    let uom = match t_hash.get(&Yaml::from_str("uom")) {
                        Some(val) => {
                            let label = val
                                .as_str()
                                .ok_or(ParseFieldError {
                                    field: format!("mappings.{}.performance_data.uom", name),
                                    kind: "string",
                                })?
                                .to_string();
                            Some(label)
                        }
                        None => None,
                    };

                    PerformanceData {
                        enabled,
                        label,
                        uom,
                    }
                }
            }
            None => PerformanceData::default(),
        }
    };

    Ok(Mapping {
        name,
        query,
        host,
        service,
        interval,
        plugin_output,
        thresholds,
        performance_data,
        last_apply: Instant::now(),
    })
}

/// Parses a multiple mappings from YAML configuration.
/// This YAML is expected to have the following format:
///
/// ```yaml
/// mappings:
///   '<first>': {} ...
///   '<second>': {} ...
///   '<third>': {} ...
///   ...
/// ```
pub(crate) fn parse_mappings(config: Hash) -> Result<Vec<Mapping>, anyhow::Error> {
    let mut mappings: Vec<Mapping> = vec![];

    match config.get(&Yaml::from_str("mappings")) {
        Some(m_raw) => {
            let mapping_hash = m_raw.as_hash().ok_or(ParseFieldError {
                field: String::from("mappings"),
                kind: "hash",
            })?;

            for raw_mapping in mapping_hash {
                let mapping = parse_mapping(raw_mapping)?;
                mappings.push(mapping);
            }

            Ok(mappings)
        }
        None => Ok(vec![]),
    }
}

fn parse_proxy_section(config: &Hash) -> Result<ProxyConfig, anyhow::Error> {
    let ignore = config
        .get(&Yaml::from_str("ignore"))
        .unwrap_or(&Yaml::Boolean(false))
        .as_bool()
        .ok_or(ParseFieldError {
            field: String::from("proxy.ignore"),
            kind: "boolean",
        })?;

    let http = match config.get(&Yaml::from_str("http")) {
        Some(val) => {
            let raw = val.as_str().ok_or(ParseFieldError {
                field: String::from("proxy.http"),
                kind: "string",
            })?;
            Some(reqwest::Proxy::http(raw)?)
        }
        None => None,
    };

    let https = match config.get(&Yaml::from_str("https")) {
        Some(val) => {
            let raw = val.as_str().ok_or(ParseFieldError {
                field: String::from("proxy.https"),
                kind: "string",
            })?;
            Some(reqwest::Proxy::https(raw)?)
        }
        None => None,
    };

    Ok(ProxyConfig {
        ignore,
        http,
        https,
    })
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

            let proxy = match prometheus.get(&Yaml::from_str("proxy")) {
                Some(p) => p
                    .as_hash()
                    .ok_or(ParseFieldError {
                        field: String::from("prometheus.proxy"),
                        kind: "hash",
                    })
                    .map_err(anyhow::Error::msg)
                    .and_then(parse_proxy_section)?,
                None => ProxyConfig::default(),
            };

            Ok(PromConfig { host, proxy })
        }
        None => Ok(PromConfig {
            host: default_host.to_string(),
            proxy: ProxyConfig::default(),
        }),
    }
}

pub(crate) fn parse_icinga_section(config: &Hash) -> Result<IcingaConfig, anyhow::Error> {
    let section = {
        let conf_attr = "icinga";
        config
            .get(&Yaml::from_str(conf_attr))
            .ok_or(MissingFieldError {
                field: String::from(conf_attr),
            })?
            .as_hash()
            .ok_or(ParseFieldError {
                field: String::from(conf_attr),
                kind: "hash",
            })?
    };

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
                .map(PathBuf::from)?,
        ),
        None => None,
    };

    let auth_hash = {
        let conf_attr = "icinga.authentication";
        section
            .get(&Yaml::from_str("authentication"))
            .ok_or(MissingFieldError {
                field: String::from(conf_attr),
            })?
            .as_hash()
            .ok_or(ParseFieldError {
                field: String::from(conf_attr),
                kind: "hash",
            })?
    };

    let auth_method = {
        let conf_attr = "icinga.authentication.method";
        auth_hash
            .get(&Yaml::from_str("method"))
            .ok_or(MissingFieldError {
                field: String::from(conf_attr),
            })?
            .as_str()
            .ok_or(ParseFieldError {
                field: String::from(conf_attr),
                kind: "string",
            })?
    };

    let authentication = match auth_method {
        "basic-auth" => {
            let username = {
                let env_var = "V2C_ICINGA_USERNAME";
                let conf_attr = "icinga.authentication.username";

                match env::var(env_var) {
                    Ok(val) => val,
                    Err(err) => {
                        debug!("failed to read Icinga ApiUser username from environment: {err}; try to read from configuration file instead");

                        auth_hash
                            .get(&Yaml::from_str("username"))
                            .ok_or(anyhow!("failed to read mandatory Icinga ApiUser username from either the environment ('{env_var}') or configuration file ('{conf_attr}')")
                            )?
                            .as_str()
                            .ok_or(ParseFieldError {
                                field: conf_attr.to_string(),
                                kind: "string",
                            })?
                            .to_string()
                    }
                }
            };

            let password = {
                let env_var = "V2C_ICINGA_PASSWORD";
                let conf_attr = "icinga.authentication.password";

                match env::var(env_var) {
                    Ok(val) => val,
                    Err(err) => {
                        debug!("failed to read Icinga ApiUser password from environment: {err}; try to read from configuration file instead");

                        auth_hash
                            .get(&Yaml::from_str("password"))
                            .ok_or(anyhow!("failed to read mandatory Icinga ApiUser username from either the environment ('{env_var}') or configuration file ('{conf_attr}')")
                            )?
                            .as_str()
                            .ok_or(ParseFieldError {
                                field: conf_attr.to_string(),
                                kind: "string",
                            })?
                            .to_string()
                    }
                }
            };

            IcingaAuth::Basic(IcingaBasicAuth { username, password })
        }
        "x509" => {
            let client_cert = {
                let conf_attr = "icinga.authentication.client_cert";
                auth_hash
                    .get(&Yaml::from_str("client_cert"))
                    .ok_or(MissingFieldError {
                        field: String::from(conf_attr),
                    })?
                    .as_str()
                    .ok_or(ParseFieldError {
                        field: String::from(conf_attr),
                        kind: "string",
                    })
                    .map(PathBuf::from)?
            };

            let client_key = {
                let conf_attr = "icinga.authentication.client_key";
                auth_hash
                    .get(&Yaml::from_str("client_key"))
                    .ok_or(MissingFieldError {
                        field: String::from(conf_attr),
                    })?
                    .as_str()
                    .ok_or(ParseFieldError {
                        field: String::from(conf_attr),
                        kind: "string",
                    })
                    .map(PathBuf::from)?
            };

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

    let proxy = match section.get(&Yaml::from_str("proxy")) {
        Some(p) => p
            .as_hash()
            .ok_or(ParseFieldError {
                field: String::from("icinga.proxy"),
                kind: "hash",
            })
            .map_err(anyhow::Error::msg)
            .and_then(parse_proxy_section)?,
        None => ProxyConfig::default(),
    };

    Ok(IcingaConfig {
        host,
        ca_cert,
        authentication,
        proxy,
    })
}

pub(crate) fn parse_yaml(source: &str) -> Result<Hash, anyhow::Error> {
    yaml_rust::yaml::YamlLoader::load_from_str(source)?[0]
        .clone()
        .into_hash()
        .ok_or(anyhow!("failed to parse configuration as hash"))
}
