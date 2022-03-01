use nagios_range::NagiosRange;
use serde::ser::{SerializeStruct, Serializer};
use serde::Serialize;
use std::collections::BTreeMap;
use std::default::Default;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// A pair of thresholds that may be provided by each mapping
/// in order to determine exit values for each time series in
/// a PromQL result set.
#[derive(Debug, Clone, Default)]
pub(crate) struct ThresholdPair {
    pub warning: Option<NagiosRange>,
    pub critical: Option<NagiosRange>,
}

/// NagiosRange does not impl Serialize, so the blanket impl does
/// not work on ThresholdPair as well.
/// The impl below will simply convert each NagiosRange to its
/// String representation that can ultimately be serialized to JSON.
impl Serialize for ThresholdPair {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut tp = serializer.serialize_struct("ThresholdPair", 2)?;
        tp.serialize_field("warning", &self.warning.map(|w| w.to_string()))?;
        tp.serialize_field("critical", &self.critical.map(|c| c.to_string()))?;
        tp.end()
    }
}

/// A single mapping built from the configuration. This contains
/// all necessary information to execute a PromQL query, process
/// the resulting set of time series and convert the data to
/// a passive check result for Icinga.
/// Note that when `mapping.service` is `None` it is assumed
/// throughout the processing of the time series that the result
/// will be used to update the state of an Icinga host object
/// instead of a service object.
#[derive(Debug, Clone)]
pub(crate) struct Mapping {
    pub name: String,
    pub query: String,
    pub thresholds: ThresholdPair,
    pub host: String,
    pub service: Option<String>,
    pub interval: Duration,
    pub last_apply: Instant,
    pub plugin_output: Option<String>,
    pub performance_data: PerformanceData,
}

/// This render context contains all information that may be accessed
/// in a handlebars template to build unique performance data labels.
/// The labels from a time series are very useful in this regard because
/// a set of time series returned by a PromQL query will in most cases
/// differ by at least one key-value pair. This pair can then be used
/// as part of a unique performance data identifier.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct PerformanceDataRenderContext<'a> {
    pub name: &'a str,
    pub host: &'a str,
    pub service: &'a Option<String>,
    pub labels: &'a BTreeMap<String, String>,
}

impl<'a> PerformanceDataRenderContext<'a> {
    pub(crate) fn from(mapping: &'a Mapping, labels: &'a BTreeMap<String, String>) -> Self {
        PerformanceDataRenderContext {
            name: &mapping.name,
            host: &mapping.host,
            service: &mapping.service,
            labels: &labels,
        }
    }
}

/// This render context contains all information that may be accessed
/// in a handlebars template to build the Icinga plugin output if the
/// generic default output does not suffice.
/// The context contains useful data from the related `Mapping`, all
/// `Data` points that were processed, the global exit value (integer)
/// and status (string, e.g. "OK") and some helper booleans.
/// Note that the helpers are serialized selectively as `is_up` does
/// not make sense in the context of an Icinga service object and
/// `is_ok` does not make sense in the context of a host object.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct PluginOutputRenderContext<'a> {
    pub name: &'a str,
    pub query: &'a str,
    pub thresholds: &'a ThresholdPair,
    pub host: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service: &'a Option<String>,
    pub interval: u64,
    pub data: &'a [Data],
    pub exit_value: &'a u8,
    pub exit_status: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_ok: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_warning: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_critical: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_up: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_down: Option<bool>,
}

impl<'a> PluginOutputRenderContext<'a> {
    pub(crate) fn from(
        mapping: &'a Mapping,
        data: &'a [Data],
        exit_value: &'a u8,
        exit_status: &'a str,
    ) -> Self {
        let updates_service = mapping.service.is_some();
        PluginOutputRenderContext {
            name: &mapping.name,
            query: &mapping.query,
            thresholds: &mapping.thresholds,
            host: &mapping.host,
            service: &mapping.service,
            interval: mapping.interval.as_secs(),
            data,
            exit_value,
            exit_status,
            is_ok: if updates_service {
                Some(*exit_value == 0)
            } else {
                None
            },
            is_warning: if updates_service {
                Some(*exit_value == 1)
            } else {
                None
            },
            is_critical: if updates_service {
                Some(*exit_value == 2)
            } else {
                None
            },
            is_up: if updates_service {
                None
            } else {
                Some(*exit_value == 0)
            },
            is_down: if updates_service {
                None
            } else {
                Some(*exit_value == 1)
            },
        }
    }
}

/// This is just a helper type that is created from the original
/// data type returned as part of the PromQL query result.
/// It only stores the label set and the value and discards
/// everything else.
/// This type is also necessary to facilitate proper unit tests as
/// `prometheus_http_query::response::InstantVector` is private.
#[derive(Debug, Clone)]
pub(crate) struct TimeSeries {
    pub labels: BTreeMap<String, String>,
    pub value: f64,
}

impl TimeSeries {
    /// Create a `TimeSeries` from `prometheus_http_query::response::InstantVector`.
    pub(crate) fn from(instant_vector: &prometheus_http_query::response::InstantVector) -> Self {
        TimeSeries {
            labels: BTreeMap::from_iter(
                instant_vector
                    .metric()
                    .into_iter()
                    .map(|(k, v)| (k.to_owned(), v.to_owned())),
            ),
            value: instant_vector.sample().value(),
        }
    }
}

/// `Data` points are computed from each time series returned by
/// a PromQL query. Each contains individual exit values and
/// status (not to be conflated with the global exit value and
/// status), the time series labels and value and some helper
/// booleans that are useful when `Data` points are rendered
/// from a handlebars template.
/// Note that the helpers are serialized selectively as `is_up` does
/// not make sense in the context of an Icinga service object and
/// `is_ok` does not make sense in the context of a host object.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub(crate) struct Data {
    pub labels: BTreeMap<String, String>,
    pub value: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_ok: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_warning: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_critical: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_up: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_down: Option<bool>,
    pub exit_status: String,
    #[serde(rename = "exit_value")]
    pub real_exit_value: u8,
    #[serde(skip_serializing)]
    pub temp_exit_value: u8,
}

impl Data {
    /// Create a `Data` point from a `TimeSeries` and additional data.
    /// The newly created data point simply extends the time
    /// series data with the "check data" that resulted from
    /// e.g. comparing the value of a time series with the
    /// configured thresholds.
    pub(crate) fn from(
        updates_service: bool,
        time_series: TimeSeries,
        real_exit_value: u8,
        temp_exit_value: u8,
        exit_status: String,
    ) -> Self {
        Data {
            labels: time_series.labels,
            value: time_series.value,
            is_ok: if updates_service {
                Some(real_exit_value == 0)
            } else {
                None
            },
            is_warning: if updates_service {
                Some(real_exit_value == 1)
            } else {
                None
            },
            is_critical: if updates_service {
                Some(real_exit_value == 2)
            } else {
                None
            },
            is_up: if updates_service {
                None
            } else {
                Some(real_exit_value == 0)
            },
            is_down: if updates_service {
                None
            } else {
                Some(real_exit_value == 1)
            },
            real_exit_value,
            temp_exit_value,
            exit_status,
        }
    }
}

pub(crate) struct PromConfig {
    pub host: String,
    pub proxy: ProxyConfig,
}

pub(crate) struct IcingaConfig {
    pub host: String,
    pub ca_cert: Option<PathBuf>,
    pub authentication: IcingaAuth,
    pub proxy: ProxyConfig,
}

pub(crate) enum IcingaAuth {
    Basic(IcingaBasicAuth),
    X509(IcingaX509Auth),
}

#[derive(Clone)]
pub(crate) struct IcingaBasicAuth {
    pub username: String,
    pub password: String,
}

pub(crate) struct IcingaX509Auth {
    pub client_cert: PathBuf,
    pub client_key: PathBuf,
}

#[derive(Default)]
pub(crate) struct ProxyConfig {
    pub ignore: bool,
    pub http: Option<reqwest::Proxy>,
    pub https: Option<reqwest::Proxy>,
}

#[derive(Debug, Clone)]
pub(crate) struct PerformanceData {
    pub enabled: bool,
    pub label: Option<String>,
    pub uom: Option<String>,
}

impl Default for PerformanceData {
    fn default() -> Self {
        PerformanceData {
            enabled: true,
            label: None,
            uom: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn test_data_init_for_host_object() {
        let mut labels = BTreeMap::new();
        labels.insert("test_label".to_string(), "test_value".to_string());
        let time_series = TimeSeries {
            labels: labels.clone(),
            value: 5.0,
        };
        let result = Data {
            labels: labels,
            value: 5.0,
            is_ok: None,
            is_warning: None,
            is_critical: None,
            is_up: Some(true),
            is_down: Some(false),
            real_exit_value: 0,
            temp_exit_value: 0,
            exit_status: "UP".to_string(),
        };
        assert_eq!(
            Data::from(false, time_series, 0, 0, "UP".to_string()),
            result
        );
    }

    #[test]
    fn test_data_init_for_service_object() {
        let mut labels = BTreeMap::new();
        labels.insert("test_label".to_string(), "test_value".to_string());
        let time_series = TimeSeries {
            labels: labels.clone(),
            value: 5.0,
        };
        let result = Data {
            labels: labels,
            value: 5.0,
            is_ok: Some(true),
            is_warning: Some(false),
            is_critical: Some(false),
            is_up: None,
            is_down: None,
            real_exit_value: 0,
            temp_exit_value: 0,
            exit_status: "OK".to_string(),
        };
        assert_eq!(
            Data::from(true, time_series, 0, 0, "OK".to_string()),
            result
        );
    }
}
