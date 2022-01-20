use nagios_range::NagiosRange;
use prometheus_http_query::Scheme;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub(crate) struct ThresholdPair {
    pub warning: Option<NagiosRange>,
    pub critical: Option<NagiosRange>,
}

#[derive(Debug)]
pub(crate) struct Mapping<'a> {
    pub name: &'a str,
    pub query: &'a str,
    pub thresholds: Option<ThresholdPair>,
    pub host: &'a str,
    pub service: &'a str,
    pub interval: Duration,
    pub last_apply: Instant,
}

pub(crate) struct PromConfig {
    pub scheme: Scheme,
    pub host: String,
    pub port: u16,
}

pub(crate) struct IcingaConfig {
    pub scheme: Scheme,
    pub host: String,
    pub port: u16,
    pub authentication: IcingaAuth,
}

pub(crate) enum IcingaAuth {
    Basic(IcingaBasicAuth),
    X509(IcingaX509Auth),
}

pub(crate) struct IcingaBasicAuth {
    pub username: String,
    pub password: String,
}

pub(crate) struct IcingaX509Auth {
    pub ca_cert: PathBuf,
    pub client_cert: PathBuf,
    pub client_key: PathBuf,
}
