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
    pub metric: &'a HashMap<String, String>,
}

impl<'a> PerformanceDataRenderContext<'a> {
    pub(crate) fn from(mapping: &'a Mapping, metric: &'a HashMap<String, String>) -> Self {
        PerformanceDataRenderContext {
            name: &mapping.name,
            host: &mapping.host,
            service: &mapping.service,
            metric: &metric,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PluginOutputRenderContext<'a> {
    pub name: &'a str,
    pub query: &'a str,
    pub thresholds: &'a ThresholdPair,
    pub host: &'a str,
    pub service: &'a Option<String>,
    pub interval: u64,
    pub data: &'a [Data<'a>],
    pub exit_status: &'a u8,
    pub state: &'a str,
}

impl<'a> PluginOutputRenderContext<'a> {
    pub(crate) fn from(
        mapping: &'a Mapping,
        data: &'a [Data<'a>],
        exit_status: &'a u8,
        state: &'a str,
    ) -> Self {
        PluginOutputRenderContext {
            name: &mapping.name,
            query: &mapping.query,
            thresholds: &mapping.thresholds,
            host: &mapping.host,
            service: &mapping.service,
            interval: mapping.interval.as_secs(),
            data: data,
            exit_status: &exit_status,
            state: &state,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Data<'a> {
    pub metric: &'a HashMap<String, String>,
    pub value: f64,
    pub is_warning: bool,
    pub is_critical: bool,
    pub state: String,
    pub exit_status: u8,
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
