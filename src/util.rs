use crate::icinga;
use crate::types::Mapping;
use anyhow::anyhow;
use anyhow::Context;
use log::{debug, warn};
use std::collections::HashMap;
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

        // Return a default plugin output and state UNKNOWN (3) when the query result is empty.
        // Also do not return performance data in this case.
        // Else process the non-empty query result.
        let (plugin_output, exit_status, performance_data) = if instant_vector.is_empty() {
            warn!("'{}': PromQL query result is empty, default to 'UNKNOWN|DOWN' status (exit code '3')", mapping.name);
            let plugin_output = icinga::plugin_output::format_default_without_result();
            let exit_status = 3;
            let performance_data = None;
            (plugin_output, exit_status, performance_data)
        } else {
            let item_count = instant_vector.len();

            let metric: Vec<&HashMap<String, String>> = instant_vector.iter().map(|item| item.metric()).collect();
            let values: Vec<f64> = instant_vector.iter().map(|item| item.sample().value()).collect();
            let exit_status = icinga::determine_exit_status(&mapping.thresholds, &values);

            let performance_data = if mapping.performance_data.enabled {
                Some(icinga::format_performance_data(&mapping, &metric, &values)?)
            } else {
                None
            };

            let plugin_output = if item_count == 1 {
                debug!("'{}': Process the one and only item in the PromQL query result set", mapping.name);
                let item = instant_vector.first().unwrap();
                let value = item.sample().value();

                if mapping.plugin_output.is_none() {
                    debug!("'{}': Use default plugin output as no custom output template is configured", mapping.name);
                    icinga::plugin_output::format_default_single_item(&mapping, value, exit_status)
                } else {
                    debug!("'{}': Process dynamic parts of custom plugin output template: {}", mapping.name, mapping.plugin_output.as_ref().unwrap());
//                    let out = icinga::plugin_output::format_plugin_output(&mapping, value, metric, exit_status)?;
//                    debug!("'{}': Use the following custom plugin output: {}", mapping.name, out);
                    //                    out
                    format!("")
                }
            } else {
                debug!("'{}': Process the PromQL query result set (total of {} items)", mapping.name, item_count);

                if mapping.plugin_output.is_none() {
                    debug!("'{}': Use default plugin output as no custom output template is configured", mapping.name);
                    icinga::plugin_output::format_default_multiple_items(&mapping, &values, exit_status)
                } else {
                    debug!("'{}': Process dynamic parts of custom plugin output template: {}", mapping.name, mapping.plugin_output.as_ref().unwrap());
//                    let out = icinga::plugin_output::format_plugin_output(&mapping, value, metric, exit_status)?;
//                    debug!("'{}': Use the following custom plugin output: {}", mapping.name, out);
//                    out
                    format!("")
                }
            };
            (plugin_output, exit_status, performance_data)
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
