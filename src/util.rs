use crate::types::Mapping;
use anyhow::Context;
use std::num::FpCategory;
use std::time::{Duration, SystemTime};

#[inline]
pub(crate) fn compute_delta(mapping: &Mapping) -> Duration {
    mapping
        .interval
        .saturating_sub(mapping.last_apply.elapsed())
}

#[inline]
pub(crate) fn get_unix_timestamp() -> Result<u64, anyhow::Error> {
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .with_context(|| "failed to retrieve current UNIX timestamp")?
        .as_secs();

    Ok(timestamp)
}

#[inline]
pub(crate) fn truncate_to_string(value: f64) -> String {
    match &value.fract().classify() {
        FpCategory::Zero => value.to_string(),
        _ => format!("{:.2}", value),
    }
}
