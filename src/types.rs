pub(crate) struct NagiosRange(pub String);

pub(crate) struct Thresholds {
    pub warning: NagiosRange,
    pub critical: NagiosRange,
}

pub(crate) struct Mapping {
    pub name: String,
    pub query: String,
    //pub thresholds: Thresholds,
    pub host: String,
    pub service: String,
}
