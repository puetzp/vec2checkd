use crate::helpers;
use crate::types::*;
use anyhow::{bail, Context};
use handlebars::Handlebars;
use log::{debug, warn};
use md5::{Digest, Md5};
use reqwest::{Certificate, Identity};
use serde::Serialize;
use std::boxed::Box;
use std::collections::HashSet;
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
    pub fn new(mut config: IcingaConfig) -> Result<Self, anyhow::Error> {
        let mut builder = match &config.authentication {
            IcingaAuth::Basic(_) => reqwest::Client::builder(),
            IcingaAuth::X509(auth) => {
                let identity = {
                    let mut buf = Vec::new();
                    debug!("Read client certificate from {:?}", auth.client_cert);
                    File::open(&auth.client_cert)
                        .with_context(|| {
                            format!(
                                "failed to read client certificate from {:?}",
                                auth.client_cert
                            )
                        })?
                        .read_to_end(&mut buf)?;

                    debug!("Read client key from {:?}", auth.client_key);
                    File::open(&auth.client_key)
                        .with_context(|| {
                            format!("failed to read client key from {:?}", auth.client_key)
                        })?
                        .read_to_end(&mut buf)?;

                    Identity::from_pem(&buf)
                        .with_context(|| "failed to parse client certificate and/or key as PEM")?
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
                debug!("Read CA certificate from {:?}", cert);
                File::open(&cert)
                    .with_context(|| format!("failed to read CA certificate from {:?}", cert))?
                    .read_to_end(&mut buf)?;
                Certificate::from_pem(&buf)
                    .with_context(|| "failed to parse CA certificate as PEM")?
            };

            builder = builder.add_root_certificate(cert_obj);
        };

        if config.proxy.ignore {
            builder = builder.no_proxy();
        } else {
            if let Some(proxy_host) = config.proxy.host {
                builder = builder.proxy(proxy_host);
            }
        }

        let client = builder.build()?;

        config.host.set_path("v1/actions/process-check-result");

        debug!(
            "Set API URL to send passive check results to {}",
            config.host
        );

        let basic_auth = match &config.authentication {
            IcingaAuth::Basic(auth) => Some(auth.clone()),
            IcingaAuth::X509(_) => None,
        };

        Ok(IcingaClient {
            client,
            url: config.host.to_string(),
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
        builder = builder.timeout(crate::util::compute_delta(mapping));

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
    performance_data: Option<Vec<String>>,
    filter: String,
    filter_vars: serde_json::Value,
    ttl: u64,
    execution_start: u64,
    execution_end: u64,
}

/// Take a mapping and all additional computed parameters and build
/// the body of the Icinga API request from it.
pub(crate) fn build_payload(
    mapping: &Mapping,
    exit_value: u8,
    plugin_output: String,
    performance_data: Option<Vec<String>>,
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
        exit_status: exit_value,
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
    pub(crate) fn format_from_template(
        template: &str,
        mapping: &Mapping,
        data: Vec<Data>,
        exit_value: u8,
        exit_status: String,
    ) -> Result<String, anyhow::Error> {
        let mut handlebars = Handlebars::new();
        handlebars.set_strict_mode(true);
        handlebars.register_helper("truncate", Box::new(helpers::truncate));
        let context = PluginOutputRenderContext::from(mapping, &data, &exit_value, &exit_status);
        let plugin_output = handlebars
            .render_template(template, &context)
            .with_context(|| {
                "failed to render plugin output from handlebars template using the given context"
            })?;
        debug!("{:?}", plugin_output);

        Ok(plugin_output)
    }

    /// Return a default plugin output corresponding to an UNKNOWN state
    /// due to an empty query result.
    #[inline]
    pub(crate) fn format_default_without_data(mapping: &str, updates_service: bool) -> String {
        if updates_service {
            warn!(
                "'{}': PromQL query result is empty, default to 'UNKNOWN' status",
                mapping
            );
            "[UNKNOWN] PromQL query result set is empty".to_string()
        } else {
            warn!(
                "'{}': PromQL query result is empty, default to 'DOWN' status",
                mapping
            );
            "[DOWN] PromQL query result set is empty".to_string()
        }
    }

    /// Return the default plugin output when the query result set contains
    /// single item.
    /// The plugin output varies a little depending on if a Icinga service name
    /// is configured or the check result targets a host object.
    #[inline]
    pub(crate) fn format_default_single_item(
        mapping: &Mapping,
        value: f64,
        exit_value: u8,
        exit_status: String,
    ) -> String {
        debug!("'{}': Build default plugin output from the one and only item in the PromQL query result set", mapping.name);
        match exit_value {
            2 => {
                // Can be unwrapped safely as exit status 2 is only possible when a
                // critical threshold was given.
                let crit_range = mapping.thresholds.critical.as_ref().unwrap().to_string();
                format!(
                    "[{}] PromQL query returned one result within the critical range ({:.2?} in {})",
                    exit_status, value, crit_range
                )
            }
            1 => {
                // Can be unwrapped safely as exit status 1 is only possible when a
                // warning threshold was given.
                let warn_range = mapping.thresholds.warning.as_ref().unwrap().to_string();
                format!(
                    "[{}] PromQL query returned one result within the warning range ({:.2?} in {})",
                    exit_status, value, warn_range
                )
            }
            0 => {
                format!(
                    "[{}] PromQL query returned one result ({:.2?})",
                    exit_status, value
                )
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
        values: &[&f64],
        exit_value: u8,
        exit_status: String,
    ) -> String {
        debug!(
            "'{}': Build default plugin output from {} items in the PromQL query result set",
            mapping.name,
            values.len()
        );

        let min_value = values.iter().map(|v| **v).reduce(f64::min).unwrap();
        let max_value = values.iter().map(|v| **v).reduce(f64::max).unwrap();
        let value_range = min_value..=max_value;

        match exit_value {
            2 => {
                // Can be unwrapped safely as exit status 2 is only possible when a
                // critical threshold was given.
                let crit_range = mapping.thresholds.critical.as_ref().unwrap().to_string();
                format!(
                    "[{}] PromQL query returned multiple results within the critical range (values {:.2?} overlap with {})",
                    exit_status, value_range, crit_range
                )
            }
            1 => {
                // Can be unwrapped safely as exit status 1 is only possible when a
                // warning threshold was given.
                let warn_range = mapping.thresholds.warning.as_ref().unwrap().to_string();
                format!(
                    "[{}] PromQL query returned multiple results within the warning range (values {:.2?} overlap with {})",
                    exit_status, value_range, warn_range
                )
            }
            0 => {
                format!(
                    "[{}] PromQL query returned multiple results in the range {:.2?}",
                    exit_status, value_range
                )
            }
            // Exit status "3"/"UNKNOWN" can be ignored safely as it has been handled
            // prior to the call to this function.
            _ => unreachable!(),
        }
    }
}

/// Check if at least one value lies in the warning/critical
/// range while the critical range takes precedence over the warning range.
/// Then two exit values are returned depending on the type of object that
/// is ultimately updated with the check result. A real value that is sent
/// as-is to the Icinga API and a temporary value that is used to provide
/// more detailed plugin output in case of host objects and is dropped later.
pub(crate) fn check_thresholds(mapping: &Mapping, value: f64) -> (u8, u8) {
    if let Some(critical) = mapping.thresholds.critical {
        if critical.check(value) {
            if mapping.service.is_some() {
                return (2, 2);
            } else {
                return (1, 2);
            }
        }
    }

    if let Some(warning) = mapping.thresholds.warning {
        if warning.check(value) {
            if mapping.service.is_some() {
                return (1, 1);
            } else {
                return (0, 1);
            }
        }
    }

    (0, 0)
}

/// Basic Nagios stuff. A particular exit status is associated with a given
/// state. The state differs for host and service objects.
pub(crate) fn exit_value_to_status(updates_service: bool, exit_value: &u8) -> String {
    match exit_value {
        3 => {
            if updates_service {
                "UNKNOWN".to_string()
            } else {
                "DOWN".to_string()
            }
        }
        2 => {
            if updates_service {
                "CRITICAL".to_string()
            } else {
                "DOWN".to_string()
            }
        }
        1 => {
            if updates_service {
                "WARNING".to_string()
            } else {
                "UP".to_string()
            }
        }
        0 => {
            if updates_service {
                "OK".to_string()
            } else {
                "UP".to_string()
            }
        }
        _ => unreachable!(),
    }
}

/// Process and return an array of performance data strings.
/// By default performance data labels are built from the mapping name and the
/// MD5-hash of the label set of each time series in the query result set.
/// This guarantees that performance data are still uniquely identifiable for
/// the backend that processes these data irregardless of the order in which
/// time series are returned by the API or the total amount of returned
/// time series.
/// Unique performance data labels can also be computed from a handlebars
/// template. That is useful when you are absolutely sure that the set of
/// time series returned by the API contains a known-to-be-unique label
/// value. Using this specific label value makes the performance data
/// more readable and less "generic".
/// Attach warning and critical thresholds to the performance data string when
/// they are configured in the mapping.
/// See https://nagios-plugins.org/doc/guidelines.html#AEN200 for the
/// expected format.
#[inline]
pub(crate) fn format_performance_data(
    mapping: &Mapping,
    data: &[Data],
) -> Result<Vec<String>, anyhow::Error> {
    let mut result = vec![];
    let mut unique_labels = HashSet::new();

    // Render performance data labels from a handlebars template.
    // As with the default label implementation the rendered string
    // must be unique (so make sure to e.g. use a unique-across-time-series
    // label).
    if let Some(ref template) = mapping.performance_data.label {
        let mut handlebars = Handlebars::new();
        handlebars.set_strict_mode(true);
        handlebars.register_helper("truncate", Box::new(helpers::truncate));

        for item in data.iter() {
            let context = PerformanceDataRenderContext::from(mapping, &item.labels);
            let label = handlebars
                .render_template(template, &context)
                .with_context(|| "failed to render performance data from handlebars template using the given context")?;
            check_label(&mut unique_labels, &label)?;
            insert_performance_data(&mut result, mapping, &label, &item.value);
        }
    } else {
        // Concatenate all label keys and values within a vector to a single
        // string, compute the MD5 checksum from it and yield the first six
        // digits of this checksum. This default presumes that this results
        // in unique (and stable across queries) performance data labels.
        for item in data.iter() {
            let checksum = {
                let metric_str = item.labels.iter().fold(String::new(), |mut acc, metric| {
                    acc.push_str(metric.0);
                    acc.push_str(metric.1);
                    acc
                });
                let mut digest = format!("{:x}", Md5::digest(&metric_str));
                digest.truncate(6);
                digest
            };
            let label = format!("{}/{}", &mapping.name, checksum);
            check_label(&mut unique_labels, &label)?;
            insert_performance_data(&mut result, mapping, &label, &item.value);
        }
    }

    Ok(result)
}

/// Make sure that performance data labels are not empty, duplicated or multi-line.
#[inline]
fn check_label(labels: &mut HashSet<String>, label: &str) -> Result<(), anyhow::Error> {
    // Empty strings can only be generated from a template, so the error message
    // refers to the template specifically.
    if label.is_empty() {
        bail!("the performance data label template produced an empty string, performance data labels must not be empty");
    }

    // Multi-line strings can only be generated from a template, so the error message
    // refers to the template specifically.
    if label.lines().count() > 1 {
        bail!("the performance data label template produced a multi-line string, performance data labels must be singe-line ");
    }

    // Duplicates could in theory be generated from either a template or the default
    // labeling implementation.
    if !labels.insert(label.to_owned()) {
        bail!("the performance data label '{}' is already present, labels must be unique within a set of performance data", label);
    }

    Ok(())
}

#[inline]
fn insert_performance_data(result: &mut Vec<String>, mapping: &Mapping, label: &str, value: &f64) {
    let perf_data = format!(
        "'{}'={}{};{};{};;",
        label,
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
    );
    result.push(perf_data);
}

#[cfg(test)]
mod tests {
    use crate::icinga::plugin_output::*;
    use crate::icinga::*;
    use crate::types::{Mapping, ThresholdPair};
    use nagios_range::NagiosRange;
    use std::collections::BTreeMap;
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
            "[DOWN] PromQL query returned one result within the critical range (15.00 in @10:20)"
                .to_string();
        assert_eq!(
            format_default_single_item(&mapping, 15.0, 2, "DOWN".to_string()),
            result
        );
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
        let result =
            "[UP] PromQL query returned one result within the warning range (5.00 in @0:10)"
                .to_string();
        assert_eq!(
            format_default_single_item(&mapping, 5.0, 1, "UP".to_string()),
            result
        );
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
        let result = "[UP] PromQL query returned one result (2.00)".to_string();
        assert_eq!(
            format_default_single_item(&mapping, 2.0, 0, "UP".to_string()),
            result
        );
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
            "[CRITICAL] PromQL query returned one result within the critical range (15.00 in @10:20)"
                .to_string();
        assert_eq!(
            format_default_single_item(&mapping, 15.0, 2, "CRITICAL".to_string()),
            result
        );
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
            "[WARNING] PromQL query returned one result within the warning range (5.00 in @0:10)"
                .to_string();
        assert_eq!(
            format_default_single_item(&mapping, 5.0, 1, "WARNING".to_string()),
            result
        );
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
        let result = "[OK] PromQL query returned one result (2.00)".to_string();
        assert_eq!(
            format_default_single_item(&mapping, 2.0, 0, "OK".to_string()),
            result
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
        let mut data = vec![];

        let labels = BTreeMap::from([
            ("some_label".to_string(), "some_value".to_string()),
            ("another_label".to_string(), "another_value".to_string()),
        ]);
        let d = Data {
            labels,
            value: 5.0,
            is_ok: Some(true),
            is_warning: Some(false),
            is_critical: Some(false),
            is_up: None,
            is_down: None,
            exit_status: "OK".to_string(),
            real_exit_value: 0,
            temp_exit_value: 0,
        };
        data.push(d);

        let labels = BTreeMap::from([
            ("foo_label".to_string(), "foo_value".to_string()),
            ("bar_label".to_string(), "bar_value".to_string()),
        ]);
        let d = Data {
            labels,
            value: 15.0,
            is_ok: Some(true),
            is_warning: Some(false),
            is_critical: Some(false),
            is_up: None,
            is_down: None,
            exit_status: "OK".to_string(),
            real_exit_value: 0,
            temp_exit_value: 0,
        };
        data.push(d);

        let labels = BTreeMap::from([
            ("test_label".to_string(), "test_value".to_string()),
            ("z_label".to_string(), "z_value".to_string()),
        ]);
        let d = Data {
            labels,
            value: 20.5,
            is_ok: Some(true),
            is_warning: Some(false),
            is_critical: Some(false),
            is_up: None,
            is_down: None,
            exit_status: "OK".to_string(),
            real_exit_value: 0,
            temp_exit_value: 0,
        };
        data.push(d);

        let result = vec![
            format!("'foobar/eaa8c4'=5;;;;"),
            format!("'foobar/6c72e2'=15;;;;"),
            format!("'foobar/c9308d'=20.5;;;;"),
        ];

        assert_eq!(format_performance_data(&mapping, &data).unwrap(), result);
    }

    #[test]
    fn test_format_performance_data_with_duplicate_label_name() {
        let mapping = Mapping {
            name: "random name".to_string(),
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
            performance_data: PerformanceData {
                enabled: true,
                label: Some("{{ name }}".to_string()),
                uom: None,
            },
        };
        let mut data = vec![];

        let labels = BTreeMap::from([
            ("some_label".to_string(), "some_value".to_string()),
            ("another_label".to_string(), "another_value".to_string()),
        ]);
        let d = Data {
            labels,
            value: 5.0,
            is_ok: Some(true),
            is_warning: Some(false),
            is_critical: Some(false),
            is_up: None,
            is_down: None,
            exit_status: "OK".to_string(),
            real_exit_value: 0,
            temp_exit_value: 0,
        };
        data.push(d);

        let labels = BTreeMap::from([
            ("foo_label".to_string(), "foo_value".to_string()),
            ("bar_label".to_string(), "bar_value".to_string()),
        ]);
        let d = Data {
            labels,
            value: 15.0,
            is_ok: Some(true),
            is_warning: Some(false),
            is_critical: Some(false),
            is_up: None,
            is_down: None,
            exit_status: "OK".to_string(),
            real_exit_value: 0,
            temp_exit_value: 0,
        };
        data.push(d);

        assert!(format_performance_data(&mapping, &data).is_err(),);
    }

    #[test]
    fn test_format_performance_data_from_result_label_set() {
        let mapping = Mapping {
            name: "random name".to_string(),
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
            performance_data: PerformanceData {
                enabled: true,
                label: Some("{{ labels.some_label }}".to_string()),
                uom: Some("%".to_string()),
            },
        };
        let mut data = vec![];

        let labels = BTreeMap::from([
            ("some_label".to_string(), "some_value".to_string()),
            ("another_label".to_string(), "another_value".to_string()),
        ]);
        let d = Data {
            labels,
            value: 5.0,
            is_ok: Some(true),
            is_warning: Some(false),
            is_critical: Some(false),
            is_up: None,
            is_down: None,
            exit_status: "OK".to_string(),
            real_exit_value: 0,
            temp_exit_value: 0,
        };
        data.push(d);

        let labels = BTreeMap::from([
            ("some_label".to_string(), "foo_value".to_string()),
            ("bar_label".to_string(), "bar_value".to_string()),
        ]);
        let d = Data {
            labels,
            value: 15.0,
            is_ok: Some(true),
            is_warning: Some(false),
            is_critical: Some(false),
            is_up: None,
            is_down: None,
            exit_status: "OK".to_string(),
            real_exit_value: 0,
            temp_exit_value: 0,
        };
        data.push(d);

        let result = vec![
            format!("'some_value'=5%;;;;"),
            format!("'foo_value'=15%;;;;"),
        ];

        assert_eq!(format_performance_data(&mapping, &data).unwrap(), result);
    }

    #[test]
    fn test_format_performance_data_from_result_label_set_with_duplicates() {
        let mapping = Mapping {
            name: "random name".to_string(),
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
            performance_data: PerformanceData {
                enabled: true,
                label: Some("{{ labels.some_label }}".to_string()),
                uom: None,
            },
        };

        let mut data = vec![];

        let labels = BTreeMap::from([
            ("some_label".to_string(), "some_value".to_string()),
            ("another_label".to_string(), "another_value".to_string()),
        ]);
        let d = Data {
            labels,
            value: 5.0,
            is_ok: Some(true),
            is_warning: Some(false),
            is_critical: Some(false),
            is_up: None,
            is_down: None,
            exit_status: "OK".to_string(),
            real_exit_value: 0,
            temp_exit_value: 0,
        };
        data.push(d);

        let labels = BTreeMap::from([
            ("some_label".to_string(), "some_value".to_string()),
            ("bar_label".to_string(), "bar_value".to_string()),
        ]);
        let d = Data {
            labels,
            value: 5.0,
            is_ok: Some(true),
            is_warning: Some(false),
            is_critical: Some(false),
            is_up: None,
            is_down: None,
            exit_status: "OK".to_string(),
            real_exit_value: 0,
            temp_exit_value: 0,
        };
        data.push(d);

        assert!(format_performance_data(&mapping, &data).is_err(),);
    }

    #[test]
    fn test_format_plugin_output_from_template() {
        let mapping = Mapping {
            name: "random name".to_string(),
            query: "up{random_label=\"random_value\"}".to_string(),
            thresholds: ThresholdPair {
                warning: None,
                critical: Some(NagiosRange::from("@10:20").unwrap()),
            },
            host: "foo".to_string(),
            service: Some("bar".to_string()),
            interval: Duration::from_secs(60),
            last_apply: Instant::now(),
            plugin_output: Some("[{{ exit_status }}] Trivial templating test; {{ data.0.labels.some_label }}; every {{ interval }} seconds".to_string()),
            performance_data: PerformanceData::default(),
        };

        let labels = BTreeMap::from([
            ("some_label".to_string(), "some_value".to_string()),
            ("another_label".to_string(), "another_value".to_string()),
        ]);
        let data_item = Data {
            labels,
            value: 5.0,
            is_ok: Some(true),
            is_warning: Some(false),
            is_critical: Some(false),
            is_up: None,
            is_down: None,
            exit_status: "OK".to_string(),
            real_exit_value: 0,
            temp_exit_value: 0,
        };

        assert_eq!(
            format_from_template(
                mapping.plugin_output.as_ref().unwrap(),
                &mapping,
                vec![data_item],
                0,
                "OK".to_string()
            )
            .unwrap(),
            "[OK] Trivial templating test; some_value; every 60 seconds".to_string()
        );
    }

    #[test]
    fn test_format_plugin_output_from_template_with_each_loop() {
        let mapping = Mapping {
            name: "random name".to_string(),
            query: "up{random_label=\"random_value\"}".to_string(),
            thresholds: ThresholdPair {
                warning: None,
                critical: Some(NagiosRange::from("@10:20").unwrap()),
            },
            host: "foo".to_string(),
            service: Some("bar".to_string()),
            interval: Duration::from_secs(60),
            last_apply: Instant::now(),
            plugin_output: Some(
                "[{{ exit_status }}] Overall bla bla
{{ #each data }}
[{{ this.exit_status }}] {{ this.labels.known_label }} is {{ truncate prec=4 this.value }}
{{ /each }}
"
                .to_string(),
            ),
            performance_data: PerformanceData::default(),
        };
        let mut data = vec![];

        let labels = BTreeMap::from([
            ("known_label".to_string(), "foo_value".to_string()),
            ("another_label".to_string(), "another_value".to_string()),
        ]);
        let d = Data {
            labels,
            value: 5.0,
            is_ok: Some(true),
            is_warning: Some(false),
            is_critical: Some(false),
            is_up: None,
            is_down: None,
            exit_status: "OK".to_string(),
            real_exit_value: 0,
            temp_exit_value: 0,
        };
        data.push(d);

        let labels = BTreeMap::from([
            ("known_label".to_string(), "bar_value".to_string()),
            ("another_label".to_string(), "another_value".to_string()),
        ]);
        let d = Data {
            labels,
            value: 15.0,
            is_ok: Some(true),
            is_warning: Some(false),
            is_critical: Some(true),
            is_up: None,
            is_down: None,
            exit_status: "CRITICAL".to_string(),
            real_exit_value: 2,
            temp_exit_value: 2,
        };
        data.push(d);

        let labels = BTreeMap::from([
            ("known_label".to_string(), "value".to_string()),
            ("another_label".to_string(), "another_value".to_string()),
        ]);
        let d = Data {
            labels,
            value: 25.55465123,
            is_ok: Some(true),
            is_warning: Some(false),
            is_critical: Some(false),
            is_up: None,
            is_down: None,
            exit_status: "OK".to_string(),
            real_exit_value: 0,
            temp_exit_value: 0,
        };
        data.push(d);

        assert_eq!(
            format_from_template(
                mapping.plugin_output.as_ref().unwrap(),
                &mapping,
                data,
                2,
                "CRITICAL".to_string()
            )
            .unwrap(),
            "[CRITICAL] Overall bla bla
[OK] foo_value is 5
[CRITICAL] bar_value is 15
[OK] value is 25.5547
"
            .to_string()
        );
    }

    #[test]
    fn test_format_plugin_output_from_template_with_each_loop_without_service() {
        let mapping = Mapping {
            name: "random name".to_string(),
            query: "up{random_label=\"random_value\"}".to_string(),
            thresholds: ThresholdPair {
                warning: None,
                critical: Some(NagiosRange::from("@10:20").unwrap()),
            },
            host: "foo".to_string(),
            service: None,
            interval: Duration::from_secs(60),
            last_apply: Instant::now(),
            plugin_output: Some(
                "[{{ exit_status }}] Overall bla bla
{{ #each data }}
[{{ this.exit_status }}] {{ this.labels.known_label }} is {{ truncate prec=2 this.value }}
{{ /each }}
"
                .to_string(),
            ),
            performance_data: PerformanceData::default(),
        };
        let mut data = vec![];

        let labels = BTreeMap::from([
            ("known_label".to_string(), "foo_value".to_string()),
            ("another_label".to_string(), "another_value".to_string()),
        ]);
        let d = Data {
            labels,
            value: 5.0,
            is_ok: None,
            is_warning: None,
            is_critical: None,
            is_up: Some(true),
            is_down: Some(false),
            exit_status: "UP".to_string(),
            real_exit_value: 0,
            temp_exit_value: 0,
        };
        data.push(d);

        let labels = BTreeMap::from([
            ("known_label".to_string(), "bar_value".to_string()),
            ("another_label".to_string(), "another_value".to_string()),
        ]);
        let d = Data {
            labels,
            value: 15.0,
            is_ok: None,
            is_warning: None,
            is_critical: None,
            is_up: Some(false),
            is_down: Some(true),
            exit_status: "DOWN".to_string(),
            real_exit_value: 1,
            temp_exit_value: 2,
        };
        data.push(d);

        let labels = BTreeMap::from([
            ("known_label".to_string(), "value".to_string()),
            ("another_label".to_string(), "another_value".to_string()),
        ]);
        let d = Data {
            labels,
            value: 25.55465123,
            is_ok: None,
            is_warning: None,
            is_critical: None,
            is_up: Some(true),
            is_down: Some(false),
            exit_status: "UP".to_string(),
            real_exit_value: 0,
            temp_exit_value: 0,
        };
        data.push(d);

        assert_eq!(
            format_from_template(
                mapping.plugin_output.as_ref().unwrap(),
                &mapping,
                data,
                2,
                "DOWN".to_string()
            )
            .unwrap(),
            "[DOWN] Overall bla bla
[UP] foo_value is 5
[DOWN] bar_value is 15
[UP] value is 25.55
"
            .to_string()
        );
    }
}
