use anyhow::anyhow;
use nagios_range::NagiosRange;

#[derive(Debug)]
pub(crate) struct ThresholdPair {
    pub warning: Option<NagiosRange>,
    pub critical: Option<NagiosRange>,
}

#[derive(Debug)]
pub(crate) struct Mapping {
    pub name: String,
    pub query: String,
    pub thresholds: Option<ThresholdPair>,
    pub host: String,
    pub service: String,
}
