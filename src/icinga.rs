use crate::error::*;
use crate::types::*;
use log::debug;
use reqwest::{Certificate, Identity};
use serde::Serialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;

/// A client to the Icinga API that can be shared across tokio tasks.
#[derive(Clone)]
pub(crate) struct IcingaClient {
    client: reqwest::Client,
    url: String,
    basic_auth: Option<IcingaBasicAuth>,
}

impl IcingaClient {
    /// Construct a new client instance. The configuration is consistent
    /// with the restrictions described by Icinga; ref:
    /// * https://icinga.com/docs/icinga-2/latest/doc/12-icinga2-api/#security
    /// * https://icinga.com/docs/icinga-2/latest/doc/12-icinga2-api/#authentication
    pub fn new(config: &IcingaConfig) -> Result<Self, anyhow::Error> {
        let mut builder = match &config.authentication {
            IcingaAuth::Basic(_) => reqwest::Client::builder(),
            IcingaAuth::X509(auth) => {
                let identity = {
                    let mut buf = Vec::new();
                    debug!("Read client certificate (PEM) from {:?}", auth.client_cert);
                    File::open(&auth.client_cert)?.read_to_end(&mut buf)?;
                    debug!("Read client key (PEM) from {:?}", auth.client_key);
                    File::open(&auth.client_key)?.read_to_end(&mut buf)?;
                    Identity::from_pem(&buf)?
                };

                reqwest::Client::builder().identity(identity)
            }
        };

        builder = builder
            .min_tls_version(reqwest::tls::Version::TLS_1_2)
            .use_rustls_tls();

        if let Some(cert) = &config.ca_cert {
            let cert_obj = {
                let mut buf = Vec::new();
                debug!("Read CA certificate (PEM) from {:?}", cert);
                File::open(&cert)?.read_to_end(&mut buf)?;
                Certificate::from_pem(&buf)?
            };

            builder = builder.add_root_certificate(cert_obj);
        };

        let client = builder.build()?;

        let url = {
            let mut tmp_url = url::Url::parse(&config.host)?.as_str().to_string();
            tmp_url.push_str("v1/actions/process-check-result");
            tmp_url
        };

        debug!("Set API URL to send passive check results to {}", url);

        let basic_auth = match &config.authentication {
            IcingaAuth::Basic(auth) => Some(auth.clone()),
            IcingaAuth::X509(_) => None,
        };

        Ok(IcingaClient {
            client,
            url,
            basic_auth,
        })
    }

    /// Build the request body and send the passive check result to Icinga.
    pub async fn send(
        &self,
        mapping: &Mapping,
        value: f64,
        metric: &HashMap<String, String>,
        exit_status: u8,
        execution_start: u64,
        execution_end: u64,
    ) -> Result<(), anyhow::Error> {
        let payload = build_payload(
            &mapping,
            value,
            metric,
            exit_status,
            execution_start,
            execution_end,
        )?;

        let body = serde_json::to_string(&payload)?;

        let mut builder = self
            .client
            .request(reqwest::Method::POST, &self.url)
            .body(body.clone())
            .header("Accept", "application/json");

        // The Basic-Auth header needs to be attached on every request
        // if this authentication method was chosen.
        if let Some(auth) = &self.basic_auth {
            builder = builder.basic_auth(&auth.username, Some(&auth.password));
        }

        // Set the request timeout to the time remaining before the
        // next check is to be executed.
        // This may need to be further reduced when checks are skipped
        // due to e.g. slow response times from the API.
        builder = builder.timeout(crate::util::compute_delta(&mapping));

        let request = builder.build()?;

        debug!("Send request with parameters: {:?}", request);
        debug!("Send request with JSON body: {}", body);
        let response = self.client.execute(request).await?;

        debug!("Process Icinga API response: {:?}", response);
        response.error_for_status()?;
        Ok(())
    }
}

impl Default for IcingaClient {
    fn default() -> Self {
        IcingaClient {
            client: reqwest::Client::new(),
            url: String::from("http://127.0.0.1:5665/v1/actions/process-check-result"),
            basic_auth: None,
        }
    }
}

/// This struct represents the expected JSON body of a request that
/// sends passive check results; ref:
/// https://icinga.com/docs/icinga-2/latest/doc/12-icinga2-api/#process-check-result
#[derive(Serialize)]
struct IcingaPayload {
    #[serde(rename = "type")]
    obj_type: String,
    exit_status: u8,
    plugin_output: String,
    filter: String,
    filter_vars: serde_json::Value,
    ttl: u64,
    execution_start: u64,
    execution_end: u64,
}

/// The basic Nagios stuff. Check if a value lies in the range or not while
/// the critical range takes precedence over the warning range.
pub(crate) fn determine_exit_status(thresholds: &ThresholdPair, value: f64) -> u8 {
    if let Some(critical) = &thresholds.critical {
        if critical.check(value) {
            return 2;
        }
    }

    if let Some(warning) = &thresholds.warning {
        if warning.check(value) {
            return 1;
        }
    }

    0
}

/// Take a mapping and all additional computed parameters and build
/// the body of the Icinga API request from it.
fn build_payload(
    mapping: &Mapping,
    value: f64,
    metric: &HashMap<String, String>,
    exit_status: u8,
    execution_start: u64,
    execution_end: u64,
) -> Result<IcingaPayload, anyhow::Error> {
    // The extra ten seconds are somewhat arbitrary. As Icinga may need a little
    // to process the check result this prevents the host or service object to
    // fall back to its default value in between check executions.
    let ttl = mapping.interval.as_secs() + 10;

    // A request may be of type "Service" or "Host" depending on if
    // a service name is provided in the config file or not.
    let (obj_type, filter, filter_vars) = match &mapping.service {
        Some(service) => {
            let filter = String::from("host.name==hostname && service.name==servicename");

            let filter_vars = serde_json::json!({
                "hostname": mapping.host,
                "servicename": service
            });

            (String::from("Service"), filter, filter_vars)
        }
        None => {
            let filter = String::from("host.name==hostname");

            let filter_vars = serde_json::json!({
                "hostname": mapping.host
            });

            (String::from("Host"), filter, filter_vars)
        }
    };

    let plugin_output = format_plugin_output(mapping, value, metric, exit_status)?;

    Ok(IcingaPayload {
        obj_type,
        filter,
        ttl,
        exit_status,
        plugin_output,
        filter_vars,
        execution_start,
        execution_end,
    })
}

/// Format the "plugin output" (in nagios-speak) by interpreting and expanding
/// the string from the configuration or return a sensible default.
fn format_plugin_output(
    mapping: &Mapping,
    value: f64,
    metric: &HashMap<String, String>,
    exit_status: u8,
) -> Result<String, anyhow::Error> {
    if let Some(template) = &mapping.plugin_output {
        // Every substring in the template that start with '$' needs
        // to be interpreted and then replaced with its proper value.
        // To that end, first identify the starting positions of each
        // eligible substring.
        let positions = template
            .char_indices()
            .filter(|(_, c)| *c == '$')
            .map(|pair| pair.0)
            .collect::<Vec<usize>>();

        let mut plugin_output = template.to_string();

        for position in positions {
            let identifier = template[position..]
                .chars()
                .take_while(|c| !c.is_whitespace())
                .collect::<String>();

            let replacement = match identifier.as_str() {
                "$metric" => metric.get("__name__").ok_or(MissingLabelError {
                    identifier: identifier.clone(),
                    label: "__name__",
                })?,
                _ => unreachable!(),
            };

            let range = position..position + identifier.len();

            plugin_output.replace_range(range, replacement);
        }

        Ok(plugin_output)
    } else {
        match &mapping.service {
            Some(_) => {
                // exit_status cannot be zero as per determine_exit_status.
                let plugin_output = match exit_status {
                    2 => format!("[CRITICAL] {} is {}", mapping.name, value),
                    1 => format!("[WARNING] {} is {}", mapping.name, value),
                    0 => format!("[OK] {} is {}", mapping.name, value),
                    _ => unreachable!(),
                };
                return Ok(plugin_output);
            }
            None => {
                // This mapping from service states to host states is consistent
                // with Icinga2's own behaviour; ref:
                // https://icinga.com/docs/icinga-2/latest/doc/03-monitoring-basics/#check-result-state-mapping
                // Also note: exit_status cannot be zero as per determine_exit_status.
                let plugin_output = match exit_status {
                    2 => format!("[DOWN] {} is {}", mapping.name, value),
                    0 | 1 => format!("[UP] {} is {}", mapping.name, value),
                    _ => unreachable!(),
                };
                return Ok(plugin_output);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Mapping;
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    #[test]
    fn test_format_plugin_output() {
        let mapping = Mapping {
            name: "foobar".to_string(),
            query: "up{random_label=\"random_value\"}".to_string(),
            thresholds: None,
            host: "foo".to_string(),
            service: None,
            interval: Duration::from_secs(60),
            last_apply: Instant::now(),
            plugin_output: Some(String::from("custom output serves me $metric ...")),
        };
        let mut metric = HashMap::new();
        metric.insert("__name__".to_string(), "good".to_string());

        let result = format_plugin_output(&mapping, 0.0, &metric, 0).unwrap();
        assert_eq!(result, String::from("custom output serves me good ..."));
    }
}
