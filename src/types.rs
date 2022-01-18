use nagios_range::NagiosRange;
use prometheus_http_query::Scheme;
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
