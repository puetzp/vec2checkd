use nagios_range::NagiosRange;
use serde::ser::{SerializeStruct, Serializer};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub(crate) struct ThresholdPair {
    pub warning: Option<NagiosRange>,
    pub critical: Option<NagiosRange>,
}

impl Default for ThresholdPair {
    fn default() -> Self {
        ThresholdPair {
            warning: None,
            critical: None,
        }
    }
}

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

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PerformanceDataRenderContext<'a> {
    pub name: &'a str,
    pub host: &'a str,
    pub service: &'a Option<String>,
    pub labels: &'a HashMap<String, String>,
}

impl<'a> PerformanceDataRenderContext<'a> {
    pub(crate) fn from(mapping: &'a Mapping, metric: &'a HashMap<String, String>) -> Self {
        PerformanceDataRenderContext {
            name: &mapping.name,
            host: &mapping.host,
            service: &mapping.service,
            labels: &metric,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PluginOutputRenderContext<'a> {
    pub name: &'a str,
    pub query: &'a str,
    pub thresholds: &'a ThresholdPair,
    pub host: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service: &'a Option<String>,
    pub interval: u64,
    pub data: &'a [Data<'a>],
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
        data: &'a [Data<'a>],
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
            data: data,
            exit_value: &exit_value,
            exit_status: &exit_status,
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

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Data<'a> {
    pub labels: &'a HashMap<String, String>,
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
    pub exit_value: u8,
}

impl<'a> Data<'a> {
    pub(crate) fn from(
        mapping: &'a Mapping,
        time_series: &'a prometheus_http_query::response::InstantVector,
        value: f64,
        exit_value: u8,
        exit_status: String,
    ) -> Self {
        let updates_service = mapping.service.is_some();
        Data {
            labels: time_series.metric(),
            value: value,
            is_ok: if updates_service {
                Some(exit_value == 0)
            } else {
                None
            },
            is_warning: if updates_service {
                Some(exit_value == 1)
            } else {
                None
            },
            is_critical: if updates_service {
                Some(exit_value == 2)
            } else {
                None
            },
            is_up: if updates_service {
                None
            } else {
                Some(exit_value == 0)
            },
            is_down: if updates_service {
                None
            } else {
                Some(exit_value == 1)
            },
            exit_value: exit_value,
            exit_status: exit_status,
        }
    }
}

pub(crate) struct PromConfig {
    pub host: String,
}

pub(crate) struct IcingaConfig {
    pub host: String,
    pub ca_cert: Option<PathBuf>,
    pub authentication: IcingaAuth,
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
