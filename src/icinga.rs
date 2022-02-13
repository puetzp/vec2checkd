use crate::error::*;
use crate::types::*;
use crate::util;
use anyhow::bail;
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
        payload: IcingaPayload,
    ) -> Result<(), anyhow::Error> {
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

        debug!(
            "'{}': Send request with parameters: {:?}",
            mapping.name, request
        );
        debug!("'{}': Send request with JSON body: {}", mapping.name, body);
        let response = self.client.execute(request).await?;

        debug!(
            "'{}': Process Icinga API response: {:?}",
            mapping.name, response
        );
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
pub(crate) struct IcingaPayload {
    #[serde(rename = "type")]
    obj_type: String,
    exit_status: u8,
    plugin_output: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    performance_data: Option<String>,
    filter: String,
    filter_vars: serde_json::Value,
    ttl: u64,
    execution_start: u64,
    execution_end: u64,
}

/// The basic Nagios stuff. Check if a value lies in the warning/critical range
/// while the critical range takes precedence over the warning range.
pub(crate) fn determine_exit_status_from_value(thresholds: &ThresholdPair, value: f64) -> u8 {
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

/// The basic Nagios stuff. Check if at least one value lies in the warning/critical
/// range while the critical range takes precedence over the warning range.
pub(crate) fn determine_exit_status_from_values(thresholds: &ThresholdPair, values: &[f64]) -> u8 {
    if let Some(critical) = &thresholds.critical {
        if values.iter().any(|v| critical.check(*v)) {
            return 2;
        }
    }

    if let Some(warning) = &thresholds.warning {
        if values.iter().any(|v| warning.check(*v)) {
            return 1;
        }
    }

    0
}

/// Take a mapping and all additional computed parameters and build
/// the body of the Icinga API request from it.
pub(crate) fn build_payload(
    mapping: &Mapping,
    exit_status: u8,
    plugin_output: String,
    performance_data: Option<String>,
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

    Ok(IcingaPayload {
        obj_type,
        filter,
        ttl,
        exit_status,
        plugin_output,
        performance_data,
        filter_vars,
        execution_start,
        execution_end,
    })
}

pub mod plugin_output {
    use super::*;

    /// Replace placeholders in the "plugin output" (in nagios-speak) by interpreting
    /// and expanding the string with parameters from the check result.
    /// Note that this behaves almost exactly like `config::preformat_plugin_output`.
    pub(crate) fn format_plugin_output(
        mapping: &Mapping,
        value: f64,
        metric: HashMap<String, String>,
        exit_status: u8,
    ) -> Result<String, anyhow::Error> {
        let mut plugin_output = mapping.plugin_output.as_ref().unwrap().clone();

        // Note that if you want to insert a label value from the
        // PromQL query result, retrieving that value may fail at
        // runtime because the closure below actually allows for
        // a greater range of valid characters than is available
        // for label values, which is [a-zA-Z_][a-zA-Z0-9_]* as per
        // the Prometheus documentation.
        while let Some((position, var_ident)) =
            plugin_output.char_indices().find(|(_, c)| *c == '$')
        {
            let mut identifier = plugin_output[position + 1..]
                .chars()
                .take_while(|c| c.is_alphabetic() || *c == '_' || *c == '.')
                .collect::<String>();

            identifier.insert(0, var_ident);

            let replacement = match identifier.as_str() {
                "$value" => util::truncate_to_string(value),
                "$state" => match &mapping.service {
                    Some(_) => match exit_status {
                        3 => "UNKNOWN".to_string(),
                        2 => "CRITICAL".to_string(),
                        1 => "WARNING".to_string(),
                        0 => "OK".to_string(),
                        _ => unreachable!(),
                    },
                    None => match exit_status {
                        2 | 3 => "DOWN".to_string(),
                        0 | 1 => "UP".to_string(),
                        _ => unreachable!(),
                    },
                },
                "$exit_status" => exit_status.to_string(),
                "$metric" => metric
                    .get("__name__")
                    .ok_or(MissingLabelError {
                        identifier: identifier.clone(),
                        label: "__name__".to_string(),
                    })?
                    .clone(),
                _ if identifier.starts_with("$labels.") => {
                    let metric_key = identifier.as_str().split_once('.').unwrap().1;
                    metric
                        .get(metric_key)
                        .ok_or(MissingLabelError {
                            identifier: identifier.clone(),
                            label: metric_key.to_string(),
                        })?
                        .clone()
                }
                _ => {
                    bail!("the plugin output placeholder '{}' is invalid", identifier)
                }
            };

            let range = position..position + identifier.len();

            plugin_output.replace_range(range, &replacement);
        }

        Ok(plugin_output)
    }

    /// Return a default plugin output corresponding to an UNKNOWN state
    /// due to an empty query result.
    #[inline]
    pub(crate) fn format_default_without_result() -> String {
        format!("[UNKNOWN] PromQL query result set is empty")
    }

    /// Return the default plugin output when the query result set contains
    /// single item.
    /// The plugin output varies a little depending on if a Icinga service name
    /// is configured or the check result targets a host object.
    #[inline]
    pub(crate) fn format_default_single_item(
        mapping: &Mapping,
        value: f64,
        exit_status: u8,
    ) -> String {
        let value = util::truncate_to_string(value);
        match exit_status {
            2 => {
                // Can be unwrapped safely as exit status 2 is only possible when a
                // critical threshold was given.
                let crit_range = mapping.thresholds.critical.as_ref().unwrap().to_string();
                let state = mapping.service.as_ref().map_or("DOWN", |_| "CRITICAL");
                format!(
                    "[{}] PromQL query returned one result within the critical range ({} in {})",
                    state, value, crit_range
                )
            }
            1 => {
                // Can be unwrapped safely as exit status 1 is only possible when a
                // warning threshold was given.
                let warn_range = mapping.thresholds.warning.as_ref().unwrap().to_string();
                let state = mapping.service.as_ref().map_or("UP", |_| "WARNING");
                format!(
                    "[{}] PromQL query returned one result within the warning range ({} in {})",
                    state, value, warn_range
                )
            }
            0 => {
                let state = mapping.service.as_ref().map_or("UP", |_| "OK");
                format!("[{}] PromQL query returned one result ({})", state, value)
            }
            // Exit status "3"/"UNKNOWN" can be ignored safely as it has been handled
            // prior to the call to this function.
            _ => unreachable!(),
        }
    }

    /// Return the default plugin output when the query result set contains
    /// multiple items.
    /// The plugin output varies a little depending on if a Icinga service name
    /// is configured or the check result targets a host object.
    #[inline]
    pub(crate) fn format_default_multiple_items(
        mapping: &Mapping,
        values: &[f64],
        exit_status: u8,
    ) -> String {
        //        let value = util::truncate_to_string(value);
        let min_value = values.iter().map(|v| *v).reduce(f64::min).unwrap();
        let max_value = values.iter().map(|v| *v).reduce(f64::max).unwrap();
        let value_range = min_value..=max_value;
        match exit_status {
            2 => {
                // Can be unwrapped safely as exit status 2 is only possible when a
                // critical threshold was given.
                let crit_range = mapping.thresholds.critical.as_ref().unwrap().to_string();
                let state = mapping.service.as_ref().map_or("DOWN", |_| "CRITICAL");
                format!(
                    "[{}] PromQL query returned multiple results within the critical range (values {:?} overlap with {})",
                    state, value_range, crit_range
                )
            }
            1 => {
                // Can be unwrapped safely as exit status 1 is only possible when a
                // warning threshold was given.
                let warn_range = mapping.thresholds.warning.as_ref().unwrap().to_string();
                let state = mapping.service.as_ref().map_or("UP", |_| "WARNING");
                format!(
                    "[{}] PromQL query returned multiple results within the warning range (values {:?} overlap with {})",
                    state, value_range, warn_range
                )
            }
            0 => {
                let state = mapping.service.as_ref().map_or("UP", |_| "OK");
                format!(
                    "[{}] PromQL query returned multiple results in the range {:?}",
                    state, value_range
                )
            }
            // Exit status "3"/"UNKNOWN" can be ignored safely as it has been handled
            // prior to the call to this function.
            _ => unreachable!(),
        }
    }
}

/// Return performance data string corresponding to this mapping and value.
/// Attach warning and critical thresholds to the performance data string when
/// they are configured in the mapping.
/// See https://nagios-plugins.org/doc/guidelines.html#AEN200 for the
/// expected format.
#[inline]
pub(crate) fn format_performance_data(mapping: &Mapping, value: f64) -> String {
    format!(
        "'{}'={}{};{};{};;",
        mapping
            .performance_data
            .label
            .as_ref()
            .unwrap_or(&mapping.name),
        value,
        mapping
            .performance_data
            .uom
            .as_ref()
            .unwrap_or(&String::new()),
        mapping
            .thresholds
            .warning
            .as_ref()
            .map(|w| w.to_string())
            .unwrap_or_default(),
        mapping
            .thresholds
            .critical
            .as_ref()
            .map(|c| c.to_string())
            .unwrap_or_default(),
    )
}

#[cfg(test)]
mod tests {
    use crate::icinga::plugin_output::*;
    use crate::icinga::*;
    use crate::types::{Mapping, ThresholdPair};
    use nagios_range::NagiosRange;
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    #[test]
    fn test_format_default_single_item_hard_host_alert() {
        let mapping = Mapping {
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
            plugin_output: None,
            performance_data: PerformanceData::default(),
        };
        let result =
            "[DOWN] PromQL query returned one result within the critical range (15 in @10:20)"
                .to_string();
        assert_eq!(format_default_single_item(&mapping, 15.0, 2), result);
    }

    #[test]
    fn test_format_default_single_item_soft_host_alert() {
        let mapping = Mapping {
            name: "foobar".to_string(),
            query: "up{random_label=\"random_value\"}".to_string(),
            thresholds: ThresholdPair {
                warning: Some(NagiosRange::from("@10").unwrap()),
                critical: Some(NagiosRange::from("@10:20").unwrap()),
            },
            host: "foo".to_string(),
            service: None,
            interval: Duration::from_secs(60),
            last_apply: Instant::now(),
            plugin_output: None,
            performance_data: PerformanceData::default(),
        };
        let result = "[UP] PromQL query returned one result within the warning range (5 in @0:10)"
            .to_string();
        assert_eq!(format_default_single_item(&mapping, 5.0, 1), result);
    }

    #[test]
    fn test_format_default_single_item_no_host_alert() {
        let mapping = Mapping {
            name: "foobar".to_string(),
            query: "up{random_label=\"random_value\"}".to_string(),
            thresholds: ThresholdPair {
                warning: Some(NagiosRange::from("@5:10").unwrap()),
                critical: Some(NagiosRange::from("@10:20").unwrap()),
            },
            host: "foo".to_string(),
            service: None,
            interval: Duration::from_secs(60),
            last_apply: Instant::now(),
            plugin_output: None,
            performance_data: PerformanceData::default(),
        };
        let result = "[UP] PromQL query returned one result (2)".to_string();
        assert_eq!(format_default_single_item(&mapping, 2.0, 0), result);
    }

    #[test]
    fn test_format_default_single_item_crit_service_alert() {
        let mapping = Mapping {
            name: "foobar".to_string(),
            query: "up{random_label=\"random_value\"}".to_string(),
            thresholds: ThresholdPair {
                warning: None,
                critical: Some(NagiosRange::from("@10:20").unwrap()),
            },
            host: "foo".to_string(),
            service: Some("bar".to_string()),
            interval: Duration::from_secs(60),
            last_apply: Instant::now(),
            plugin_output: None,
            performance_data: PerformanceData::default(),
        };
        let result =
            "[CRITICAL] PromQL query returned one result within the critical range (15 in @10:20)"
                .to_string();
        assert_eq!(format_default_single_item(&mapping, 15.0, 2), result);
    }

    #[test]
    fn test_format_default_single_item_warn_service_alert() {
        let mapping = Mapping {
            name: "foobar".to_string(),
            query: "up{random_label=\"random_value\"}".to_string(),
            thresholds: ThresholdPair {
                warning: Some(NagiosRange::from("@10").unwrap()),
                critical: Some(NagiosRange::from("@10:20").unwrap()),
            },
            host: "foo".to_string(),
            service: Some("bar".to_string()),
            interval: Duration::from_secs(60),
            last_apply: Instant::now(),
            plugin_output: None,
            performance_data: PerformanceData::default(),
        };
        let result =
            "[WARNING] PromQL query returned one result within the warning range (5 in @0:10)"
                .to_string();
        assert_eq!(format_default_single_item(&mapping, 5.0, 1), result);
    }

    #[test]
    fn test_format_default_single_item_no_service_alert() {
        let mapping = Mapping {
            name: "foobar".to_string(),
            query: "up{random_label=\"random_value\"}".to_string(),
            thresholds: ThresholdPair {
                warning: Some(NagiosRange::from("@5:10").unwrap()),
                critical: Some(NagiosRange::from("@10:20").unwrap()),
            },
            host: "foo".to_string(),
            service: Some("bar".to_string()),
            interval: Duration::from_secs(60),
            last_apply: Instant::now(),
            plugin_output: None,
            performance_data: PerformanceData::default(),
        };
        let result = "[OK] PromQL query returned one result (2)".to_string();
        assert_eq!(format_default_single_item(&mapping, 2.0, 0), result);
    }

    #[test]
    fn test_format_plugin_output_with_threshold_and_value_and_state() {
        let mapping = Mapping {
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
                "[$state] Result value is $value (critical at: '@10:20')",
            )),
            performance_data: PerformanceData::default(),
        };
        // Test with a fractional number first.
        let result = format_plugin_output(&mapping, 5.5, HashMap::new(), 0).unwrap();
        assert_eq!(
            result,
            String::from("[UP] Result value is 5.50 (critical at: '@10:20')")
        );
        // Then test without the fractional, i.e. fractional is zero.
        let result = format_plugin_output(&mapping, 5.0, HashMap::new(), 0).unwrap();
        assert_eq!(
            result,
            String::from("[UP] Result value is 5 (critical at: '@10:20')")
        );
    }

    #[test]
    fn test_format_plugin_output_with_label_value() {
        let mapping = Mapping {
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
                "I need that $labels.random_label in my output and the metric value '$metric' while we're at it",
            )),
            performance_data: PerformanceData::default(),
        };
        let mut metric = HashMap::new();
        metric.insert("__name__".to_string(), "up".to_string());
        metric.insert("random_label".to_string(), "random_value".to_string());

        let result = format_plugin_output(&mapping, 0.0, metric, 0).unwrap();
        assert_eq!(
            result,
            String::from(
                "I need that random_value in my output and the metric value 'up' while we're at it"
            )
        );
    }

    #[test]
    fn test_format_performance_data() {
        let mapping = Mapping {
            name: "foobar".to_string(),
            query: "up{random_label=\"random_value\"}".to_string(),
            thresholds: ThresholdPair {
                warning: None,
                critical: None,
            },
            host: "foo".to_string(),
            service: None,
            interval: Duration::from_secs(60),
            last_apply: Instant::now(),
            plugin_output: None,
            performance_data: PerformanceData::default(),
        };
        let value = 5.0;
        let result = format!("'foobar'=5;;;;");
        assert_eq!(format_performance_data(&mapping, value), result);

        let value = 5.5;
        let result = format!("'foobar'=5.5;;;;");
        assert_eq!(format_performance_data(&mapping, value), result);
    }

    #[test]
    fn test_format_performance_data_with_thresholds() {
        let mapping = Mapping {
            name: "foobar".to_string(),
            query: "up{random_label=\"random_value\"}".to_string(),
            thresholds: ThresholdPair {
                warning: Some(NagiosRange::from("@10").unwrap()),
                critical: Some(NagiosRange::from("@100").unwrap()),
            },
            host: "foo".to_string(),
            service: None,
            interval: Duration::from_secs(60),
            last_apply: Instant::now(),
            plugin_output: None,
            performance_data: PerformanceData::default(),
        };
        let value = 5.5;
        let result = format!("'foobar'=5.5;@0:10;@0:100;;");
        assert_eq!(format_performance_data(&mapping, value), result);
    }

    #[test]
    fn test_format_performance_data_with_thresholds_and_uom() {
        let mapping = Mapping {
            name: "foobar".to_string(),
            query: "up{random_label=\"random_value\"}".to_string(),
            thresholds: ThresholdPair {
                warning: Some(NagiosRange::from("@10").unwrap()),
                critical: Some(NagiosRange::from("@100").unwrap()),
            },
            host: "foo".to_string(),
            service: None,
            interval: Duration::from_secs(60),
            last_apply: Instant::now(),
            plugin_output: None,
            performance_data: PerformanceData {
                enabled: true,
                label: Some("alternative".to_string()),
                uom: Some("c".to_string()),
            },
        };
        let value = 5.5;
        let result = format!("'alternative'=5.5c;@0:10;@0:100;;");
        assert_eq!(format_performance_data(&mapping, value), result);
    }
}
