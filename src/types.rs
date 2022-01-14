use nagios_range::NagiosRange;

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
}
