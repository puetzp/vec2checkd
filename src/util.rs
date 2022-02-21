use crate::icinga;
use crate::types::{Data, Mapping};
use anyhow::anyhow;
use anyhow::Context;
use log::debug;
use prometheus_http_query::response::InstantVector;
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

/// Converts a f64 to a String and truncates it at two decimals
/// if there is a fractional part to increase readability of
/// e.g. the default plugin outputs.
#[inline]
pub(crate) fn truncate_to_string(value: f64) -> String {
    match &value.fract().classify() {
        FpCategory::Zero => value.to_string(),
        _ => format!("{:.2}", value),
    }
}

/// Take the PromQL query result (set of time series) and convert them to
/// `Data` points. These data are just enriched presentations of each time
/// series containing exit values and status that are relevant to process
/// a final passive check result.
#[inline]
fn convert_query_result<'a>(
    mapping: &'a Mapping,
    instant_vectors: &'a [InstantVector],
) -> Vec<Data<'a>> {
    let mut result = vec![];

    for instant_vector in instant_vectors {
        let value = instant_vector.sample().value();
        let (real_exit_value, temp_exit_value) = icinga::check_thresholds(mapping, value);
        let exit_status = icinga::exit_value_to_status(mapping.service.as_ref(), &temp_exit_value);
        let data = Data::from(
            mapping.service.is_some(),
            instant_vector.metric(),
            value,
            real_exit_value,
            temp_exit_value,
            exit_status,
        );
        result.push(data);
    }

    result
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
        let exec_start = get_unix_timestamp()
            .with_context(|| "failed to retrieve UNIX timestamp to measure event execution")?;

        debug!(
            "'{}': start processing mapping at {}",
            mapping.name, exec_start
        );

        let prom_query = mapping.query.to_string();

        debug!("'{}': execute PromQL query '{}'", mapping.name, prom_query);

        let vector = prometheus_http_query::InstantVector(prom_query);

        let query_result = prom_client
            .query(vector, None, None)
            .await
            .with_context(|| "failed to execute PromQL query")?;

        let instant_vectors = query_result.as_instant().ok_or(anyhow!(
            "failed to parse PromQL query result as instant vector"
        ))?;

        // Return a default plugin output without performance data when the query result is empty:
        //  - UNKNOWN (3) for service objects
        //  - DOWN (1) for host objects
        // Else process the non-empty query result.
        let (plugin_output, overall_exit_value, performance_data) = if instant_vectors.is_empty() {
            let updates_service = mapping.service.is_some();
            let plugin_output =
                icinga::plugin_output::format_default_without_data(&mapping.name, updates_service);
            let overall_exit_value = if updates_service { 3 } else { 1 };
            let performance_data = None;
            (plugin_output, overall_exit_value, performance_data)
        } else {
            // Process real and temporary exit values and exit status for each time series in
            // the query result set and store them together in a structure.
            let data: Vec<Data> = convert_query_result(&mapping, instant_vectors);

            // Compute the performance data corresponding to each time series.
            let performance_data = if mapping.performance_data.enabled {
                Some(icinga::format_performance_data(&mapping, &data)?)
            } else {
                None
            };

            // This is the "real" exit value that is ultimately sent as part of the
            // payload sent to the Icinga API. The exit value is the highest from the
            // set of all individual "real" exit values that were computed for each
            // data point.
            let overall_real_exit_value = data
                .iter()
                .max_by(|x, y| x.real_exit_value.cmp(&y.real_exit_value))
                .unwrap()
                .real_exit_value;

            // This is a "temporary" exit value computed from the highest from
            // the set of all individual "temp" exit values of each data point.
            // The "temp" exit value is the same as the "real" exit value when
            // the mapping corresponds to an Icinga service object but differs
            // for host objects. That is because the service states (0-3) are
            // in this case collapsed to two states (0 and 1) but we need the
            // full range of states (0-3) to produce a more meaningful output
            // that is aware of possibly breached warning and critical thresholds.
            // As such the "temp" value is dropped after computing the default
            // output.
            let overall_temp_exit_value = data
                .iter()
                .max_by(|x, y| x.temp_exit_value.cmp(&y.temp_exit_value))
                .unwrap()
                .temp_exit_value;

            // Compute a plugin output either from a handlebars template (if any) or
            // fall back to generic default outputs.
            let plugin_output = if let Some(ref template) = mapping.plugin_output {
                debug!(
                    "'{}': Build the plugin output from the following handlebars template: {}",
                    mapping.name, template
                );
                icinga::plugin_output::format_from_template(
                    template,
                    &mapping,
                    data,
                    overall_real_exit_value,
                    overall_temp_exit_value,
                )?
            } else {
                if data.len() == 1 {
                    let value = data.first().unwrap().value;
                    icinga::plugin_output::format_default_single_item(
                        &mapping,
                        value,
                        overall_temp_exit_value,
                    )
                } else {
                    let values: Vec<&f64> = data.iter().map(|d| &d.value).collect();
                    icinga::plugin_output::format_default_multiple_items(
                        &mapping,
                        &values,
                        overall_temp_exit_value,
                    )
                }
            };
            (plugin_output, overall_real_exit_value, performance_data)
        };

        let exec_end = get_unix_timestamp()
            .with_context(|| "failed to retrieve UNIX timestamp to measure event execution")?;

        // Build the JSON payload to be sent to the Icinga API.
        // Note that the exit value is the "real" one as the API
        // returns HTTP 400 Bad Request when host states (= exit values)
        // other than 0 or 1 are sent.
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
            .send(&mapping, payload)
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
