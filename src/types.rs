use nagios_range::NagiosRange;
use prometheus_http_query::Scheme;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub(crate) struct ThresholdPair {
    pub warning: Option<NagiosRange>,
    pub critical: Option<NagiosRange>,
}

#[derive(Debug, Clone)]
pub(crate) struct Mapping {
    pub name: String,
    pub query: String,
    pub thresholds: Option<ThresholdPair>,
    pub host: String,
    pub service: Option<String>,
    pub interval: Duration,
    pub last_apply: Instant,
}

pub(crate) struct PromConfig {
    pub host: String,
}

pub(crate) struct IcingaConfig {
    pub scheme: Scheme,
    pub host: String,
    pub port: u16,
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
