use crate::icinga;
use crate::types::{Data, Mapping};
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

        // Return a default plugin output without performance data when the query result is empty:
        //  - UNKNOWN (3) for service objects
        //  - DOWN (1) for host objects
        // Else process the non-empty query result.
        let (plugin_output, overall_exit_value, performance_data) = if instant_vector.is_empty() {
            let updates_service = mapping.service.is_some();
            let plugin_output = icinga::plugin_output::format_default(&mapping.name, updates_service);
            let overall_exit_value = if updates_service { 3 } else { 1 };
            let performance_data = None;
            (plugin_output, overall_exit_value, performance_data)
        } else {
            let data: Vec<Data> = instant_vector.into_iter().map(|ts| {
                // Discard the timestamp that is also part of a sample.
                let value = ts.sample().value();
                // Compute the exit status per time series, i.e. if the value breaches any thresholds.
                let (real_exit_value, temp_exit_value) = icinga::check_thresholds(&mapping, value);
                let exit_status = icinga::exit_value_to_status(mapping.service.as_ref(), &real_exit_value);
                Data::from(&mapping, ts, value, real_exit_value, temp_exit_value, exit_status)
            }).collect();

            let performance_data = if mapping.performance_data.enabled {
                Some(icinga::format_performance_data(&mapping, &data)?)
            } else {
                None
            };

            let overall_temp_exit_value = data.iter().max_by(|x, y| x.temp_exit_value.cmp(&y.temp_exit_value)).unwrap().temp_exit_value;

            let overall_real_exit_value = data.iter().max_by(|x, y| x.real_exit_value.cmp(&y.real_exit_value)).unwrap().real_exit_value;

            let plugin_output = if let Some(ref template) = mapping.plugin_output {
                debug!("'{}': Build the plugin output from the following handlebars template: {}", mapping.name, template);
                icinga::plugin_output::format_from_template(&template, &mapping, data, overall_real_exit_value)?
            } else {
                let item_count = data.len();
                if item_count == 1 {
                    debug!("'{}': Build default plugin output from the one and only item in the PromQL query result set", mapping.name);
                    let value = data.first().unwrap().value;
                    icinga::plugin_output::format_default_single_item(&mapping, value, overall_temp_exit_value)
                } else {
                    debug!("'{}': Build default plugin output from {} items in the PromQL query result set", mapping.name, item_count);
                    let values: Vec<&f64> = data.iter().map(|d| &d.value).collect();
                    icinga::plugin_output::format_default_multiple_items(&mapping, &values, overall_temp_exit_value)
                }
            };
            (plugin_output, overall_real_exit_value, performance_data)
        };

        let exec_end = get_unix_timestamp().with_context(|| {
            "failed to retrieve UNIX timestamp to measure event execution"
        })?;

        let payload = icinga::build_payload(
            &mapping,
            overall_exit_value,
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
