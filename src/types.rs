use anyhow::anyhow;
use nagios_range::NagiosRange;

pub(crate) struct ThresholdPair {
    pub warning: NagiosRange,
    pub critical: NagiosRange,
}

pub(crate) struct Mapping {
    pub name: String,
    pub query: String,
    pub thresholds: ThresholdPair,
    pub host: String,
    pub service: String,
}
