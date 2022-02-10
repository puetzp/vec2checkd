use crate::icinga;
use crate::types::Mapping;
use anyhow::anyhow;
use anyhow::Context;
use log::{debug, warn};
use std::num::FpCategory;
use std::time::{Duration, SystemTime};

type TaskResult = Result<Result<(), anyhow::Error>, tokio::task::JoinError>;

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

/// This function performs all necessary steps to execute a PromQL query, process
/// the query result, transform it to a passive check result and send it to Icinga.
/// The result of this operation including any errors that may have occured in the
/// process are returned to the calling function.
pub(crate) async fn execute_task(
    prom_client: prometheus_http_query::Client,
    icinga_client: icinga::IcingaClient,
    mapping: Mapping,
) -> TaskResult {
    tokio::spawn(async move {
        let exec_start = get_unix_timestamp().with_context(|| {
            "failed to retrieve UNIX timestamp to measure event execution"
        })?;

        debug!(
            "'{}': start processing mapping at {}",
            mapping.name, exec_start
        );

        let prom_query = mapping.query.to_string();

        debug!(
            "'{}': execute PromQL query '{}'",
            mapping.name, prom_query
        );

        let vector = prometheus_http_query::InstantVector(prom_query);

        let abstract_vector = prom_client
            .query(vector, None, None)
            .await
            .with_context(|| "failed to execute PromQL query")?;

        let instant_vector = abstract_vector
            .as_instant()
            .ok_or(anyhow!(
                "failed to parse PromQL query result as instant vector"
            ))?;

        // Get the first item of the PromQL query result set and apply all needed operations.
        // Ignore any customizations etc. when the result set is empty and default to
        // UNKNOWN/DOWN state with a static output.
        let (plugin_output, exit_status, performance_data) = match instant_vector.get(0) {
            Some(first_vec) => {
                debug!("'{}': Process only the first item from the PromQL vector result set", mapping.name);
                let value = first_vec.sample().value();
                let metric = first_vec.metric().clone();
                let exit_status = icinga::determine_exit_status(&mapping.thresholds, value);

                let plugin_output = if mapping.plugin_output.is_none() {
                    debug!("'{}': Use default plugin output as no custom output template is configured", mapping.name);
                    icinga::default_plugin_output(&mapping, value, exit_status)
                } else {
                    debug!("'{}': Process dynamic parts of custom plugin output template: {}", mapping.name, mapping.plugin_output.as_ref().unwrap());
                    let out = icinga::format_plugin_output(&mapping, value, metric, exit_status)?;
                    debug!("'{}': Use the following custom plugin output: {}", mapping.name, out);
                    out
                };

                let performance_data = if mapping.performance_data.enabled {
                    Some(icinga::format_performance_data(&mapping, value))
                } else {
                    None
                };

                (plugin_output, exit_status, performance_data)
            },
            None => {
                warn!("'{}': PromQL query result is empty, default to 'UNKNOWN|DOWN' status (exit code '3')", mapping.name);
                let value = 0.0;
                let exit_status = 3;
                let plugin_output = icinga::default_plugin_output(&mapping, value, exit_status);
                let performance_data = if mapping.performance_data.enabled {
                    Some(icinga::format_performance_data(&mapping, value))
                } else {
                    None
                };

                (plugin_output, exit_status, performance_data)
            }
        };

        let exec_end = get_unix_timestamp().with_context(|| {
            "failed to retrieve UNIX timestamp to measure event execution"
        })?;

        let payload = icinga::build_payload(
            &mapping,
            exit_status,
            plugin_output,
            performance_data,
            exec_start,
            exec_end,
        )?;

        debug!(
            "'{}': stop measuring processing of mapping at {}",
            mapping.name, exec_end
        );

        icinga_client
            .send(
                &mapping,
                payload
            )
            .await
            .with_context(|| "failed to send passive check result to Icinga")?;

        debug!(
            "'{}': passive check result was successfully send to Icinga",
            mapping.name
        );

        Ok(())
    })
    .await
}
