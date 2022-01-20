use crate::error::*;
use crate::types::*;
use anyhow::{anyhow, bail};
use nagios_range::NagiosRange;
use prometheus_http_query::Scheme;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use yaml_rust::yaml::{Hash, Yaml};

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

    let interval: Duration = match items.get(&Yaml::from_str("interval")) {
        Some(i) => {
            let num = i.as_i64().ok_or(ParseFieldError {
                field: format!("mappings.{}.interval", name),
                kind: "number",
            })?;

            let conv = u16::try_from(num).map_err(|_| ParseFieldError {
                field: format!("mappings.{}.interval", name),
                kind: "number",
            })?;
            Duration::from_secs(conv as u64)
        }
        None => Duration::from_secs(300),
    };

    Ok(Mapping {
        name,
        query,
        host,
        service,
        interval,
        thresholds: threshold_pair,
        last_apply: Instant::now(),
    })
}

pub(crate) fn parse_mappings<'a>(config: &'a Hash) -> Result<Vec<Mapping<'a>>, anyhow::Error> {
    let mut mappings: Vec<Mapping> = vec![];

    match config.get(&Yaml::from_str("mappings")) {
        Some(m_raw) => {
            let mapping_hash = m_raw.as_hash().ok_or(ParseFieldError {
                field: String::from("mappings"),
                kind: "hash",
            })?;

            for raw_mapping in mapping_hash {
                let parsed = parse_mapping(raw_mapping)?;
                mappings.push(parsed);
            }

            Ok(mappings)
        }
        None => Ok(vec![]),
    }
}

pub(crate) fn parse_prom_section(config: &Hash) -> Result<Option<PromConfig>, anyhow::Error> {
    let section = match config.get(&Yaml::from_str("prometheus")) {
        Some(prometheus) => prometheus.as_hash().ok_or(ParseFieldError {
            field: String::from("prometheus"),
            kind: "hash",
        })?,
        None => return Ok(None),
    };

    let scheme = match section
        .get(&Yaml::from_str("scheme"))
        .unwrap_or(&Yaml::from_str("http"))
        .as_str()
        .ok_or(ParseFieldError {
            field: String::from("prometheus.scheme"),
            kind: "string",
        })? {
        "http" => Scheme::Http,
        "https" => Scheme::Https,
        _ => {
            bail!("invalid value in 'prometheus.scheme', must be either 'http' or 'https'")
        }
    };

    let host = section
        .get(&Yaml::from_str("host"))
        .unwrap_or(&Yaml::from_str("127.0.0.1"))
        .as_str()
        .ok_or(ParseFieldError {
            field: String::from("prometheus.host"),
            kind: "string",
        })?
        .to_string();

    let port = section
        .get(&Yaml::from_str("port"))
        .unwrap_or(&Yaml::Integer(9990))
        .as_i64()
        .ok_or(ParseFieldError {
            field: String::from("prometheus.port"),
            kind: "number",
        })?;

    let port = u16::try_from(port).map_err(|_| ParseFieldError {
        field: String::from("prometheus.port"),
        kind: "number",
    })?;

    Ok(Some(PromConfig { scheme, host, port }))
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

    let scheme = match section
        .get(&Yaml::from_str("scheme"))
        .unwrap_or(&Yaml::from_str("http"))
        .as_str()
        .ok_or(ParseFieldError {
            field: String::from("icinga.scheme"),
            kind: "string",
        })? {
        "http" => Scheme::Http,
        "https" => Scheme::Https,
        _ => {
            return Err(anyhow!(
                "invalid value in 'icinga.scheme', must be either 'http' or 'https'"
            ))
        }
    };

    let host = section
        .get(&Yaml::from_str("host"))
        .unwrap_or(&Yaml::from_str("127.0.0.1"))
        .as_str()
        .ok_or(ParseFieldError {
            field: String::from("icinga.host"),
            kind: "string",
        })?
        .to_string();

    let port = section
        .get(&Yaml::from_str("port"))
        .unwrap_or(&Yaml::Integer(5665))
        .as_i64()
        .ok_or(ParseFieldError {
            field: String::from("icinga.port"),
            kind: "number",
        })?;

    let port = u16::try_from(port).map_err(|_| ParseFieldError {
        field: String::from("icinga.port"),
        kind: "number",
    })?;

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
        scheme,
        host,
        port,
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
