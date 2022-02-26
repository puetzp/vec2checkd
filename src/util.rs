use crate::icinga;
use crate::types::{Data, Mapping, TimeSeries};
use anyhow::anyhow;
use anyhow::Context;
use log::debug;
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

/// Convert each time series to a set of data points that contains the
/// complete time series data and additional "check data" on top, i.e.
/// exit value, exit status and some helper variables that are useful
/// in the context of templating.
#[inline]
fn process_time_series(mapping: &Mapping, time_series: Vec<TimeSeries>) -> Vec<Data> {
    time_series
        .into_iter()
        .map(|ts| {
            let value = ts.value;
            let (real_exit_value, temp_exit_value) = icinga::check_thresholds(mapping, value);
            let updates_service = mapping.service.is_some();
            let exit_status = icinga::exit_value_to_status(updates_service, &temp_exit_value);
            Data::from(
                updates_service,
                ts,
                real_exit_value,
                temp_exit_value,
                exit_status,
            )
        })
        .collect::<Vec<Data>>()
}

/// Convert a PromQL query result (array of instant vectors/time series) to the three major parts
/// that make up an Icinga check result: the plugin output, exit value and optionally
/// an array of performance data.
fn process_query_result(
    mapping: &Mapping,
    time_series: Vec<TimeSeries>,
) -> Result<(String, u8, Option<Vec<String>>), anyhow::Error> {
    // Process real and temporary exit values and exit status for each time series in
    // the query result set and store them together in a structure.
    let data: Vec<Data> = process_time_series(mapping, time_series);

    // Compute the performance data corresponding to each time series.
    let performance_data = if mapping.performance_data.enabled {
        Some(icinga::format_performance_data(mapping, &data)?)
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

    // The overall exit status associated with the "temporary exit value".
    // One of "OK", "CRITICAL, "WARNING", "UNKNOWN" for Icinga services.
    // One of "UP", "DOWN" for Icinga hosts.
    let overall_exit_status =
        icinga::exit_value_to_status(mapping.service.is_some(), &overall_temp_exit_value);

    // Compute a plugin output either from a handlebars template (if any) or
    // fall back to generic default outputs.
    let plugin_output = match mapping.plugin_output {
        Some(ref template) => {
            debug!(
                "'{}': Build the plugin output from the following handlebars template: {}",
                mapping.name, template
            );
            icinga::plugin_output::format_from_template(
                template,
                mapping,
                data,
                overall_real_exit_value,
                overall_exit_status,
            )?
        }
        None => {
            if data.len() == 1 {
                let value = data.first().unwrap().value;
                icinga::plugin_output::format_default_single_item(
                    mapping,
                    value,
                    overall_temp_exit_value,
                    overall_exit_status,
                )
            } else {
                let values: Vec<&f64> = data.iter().map(|d| &d.value).collect();
                icinga::plugin_output::format_default_multiple_items(
                    mapping,
                    &values,
                    overall_temp_exit_value,
                    overall_exit_status,
                )
            }
        }
    };
    Ok((plugin_output, overall_real_exit_value, performance_data))
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
            let time_series: Vec<TimeSeries> =
                instant_vectors.iter().map(TimeSeries::from).collect();
            process_query_result(&mapping, time_series)?
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    use std::collections::BTreeMap;
    use std::default::Default;
    use std::time::Instant;

    fn seed_labels() -> Vec<BTreeMap<String, String>> {
        let mut label_set = vec![];
        let labels = BTreeMap::from([
            ("cluster".to_string(), "production".to_string()),
            ("condition".to_string(), "DiskPressure".to_string()),
            ("node".to_string(), "worker-01".to_string()),
            ("namespace".to_string(), "monitoring".to_string()),
            ("status".to_string(), "true".to_string()),
        ]);
        label_set.push(labels);

        label_set.push({
            BTreeMap::from([
                ("cluster".to_string(), "production".to_string()),
                ("condition".to_string(), "DiskPressure".to_string()),
                ("node".to_string(), "worker-02".to_string()),
                ("namespace".to_string(), "monitoring".to_string()),
                ("status".to_string(), "true".to_string()),
            ])
        });

        label_set.push({
            BTreeMap::from([
                ("cluster".to_string(), "production".to_string()),
                ("condition".to_string(), "MemoryPressure".to_string()),
                ("node".to_string(), "worker-01".to_string()),
                ("namespace".to_string(), "monitoring".to_string()),
                ("status".to_string(), "true".to_string()),
            ])
        });

        label_set.push({
            BTreeMap::from([
                ("cluster".to_string(), "production".to_string()),
                ("condition".to_string(), "MemoryPressure".to_string()),
                ("node".to_string(), "worker-02".to_string()),
                ("namespace".to_string(), "monitoring".to_string()),
                ("status".to_string(), "true".to_string()),
            ])
        });

        label_set.push({
            BTreeMap::from([
                ("cluster".to_string(), "production".to_string()),
                ("condition".to_string(), "NetworkUnavailable".to_string()),
                ("node".to_string(), "worker-01".to_string()),
                ("namespace".to_string(), "monitoring".to_string()),
                ("status".to_string(), "true".to_string()),
            ])
        });

        label_set.push({
            BTreeMap::from([
                ("cluster".to_string(), "production".to_string()),
                ("condition".to_string(), "NetworkUnavailable".to_string()),
                ("node".to_string(), "worker-02".to_string()),
                ("namespace".to_string(), "monitoring".to_string()),
                ("status".to_string(), "true".to_string()),
            ])
        });

        label_set.push({
            BTreeMap::from([
                ("cluster".to_string(), "production".to_string()),
                ("condition".to_string(), "PIDPressure".to_string()),
                ("node".to_string(), "worker-01".to_string()),
                ("namespace".to_string(), "monitoring".to_string()),
                ("status".to_string(), "true".to_string()),
            ])
        });

        label_set.push({
            BTreeMap::from([
                ("cluster".to_string(), "production".to_string()),
                ("condition".to_string(), "PIDPressure".to_string()),
                ("node".to_string(), "worker-02".to_string()),
                ("namespace".to_string(), "monitoring".to_string()),
                ("status".to_string(), "true".to_string()),
            ])
        });
        label_set
    }

    #[test]
    fn test_process_query_result() {
        let label_set = seed_labels();
        let values = [0.0, 0.0, 0.0, 5.1238712, 0.0, 0.0, 0.0, 0.0];
        let time_series = label_set
            .iter()
            .zip(values.iter())
            .map(|(labels, value)| TimeSeries {
                labels: labels.clone(),
                value: *value,
            })
            .collect::<Vec<TimeSeries>>();

        let mut mapping = Mapping {
            name: "Node status".to_string(),
            query: r#"kube_node_status_condition{cluster="production",condition!="Ready",status="true"}"#.to_string(),
            thresholds: ThresholdPair {
                warning: None,
                critical: None,
            },
            host: "foo".to_string(),
            service: Some("bar".to_string()),
            interval: Duration::from_secs(60),
            last_apply: Instant::now(),
            plugin_output: None,
            performance_data: PerformanceData {
                enabled: false,
                ..Default::default()
            },
        };

        let expected_output =
            "[OK] PromQL query returned multiple results in the range 0.00..=5.12".to_string();

        // Test: Default output, multiple time series, OK, service object, no performance data.
        assert_eq!(
            process_query_result(&mapping, time_series.clone()).unwrap(),
            (expected_output, 0, None)
        );

        mapping.service = None;
        let expected_output =
            "[UP] PromQL query returned multiple results in the range 0.00..=5.12".to_string();

        // Test: Default output, multiple time series, OK, host object, no performance data.
        assert_eq!(
            process_query_result(&mapping, time_series.clone()).unwrap(),
            (expected_output, 0, None)
        );

        let time_series = vec![TimeSeries {
            labels: label_set[0].clone(),
            value: 12.34534534,
        }];

        let expected_output = "[UP] PromQL query returned one result (12.35)".to_string();

        // Test: Default output, single time series, OK, host object, no performance data.
        assert_eq!(
            process_query_result(&mapping, time_series.clone()).unwrap(),
            (expected_output, 0, None)
        );

        mapping.service = Some("bar".to_string());
        let expected_output = "[OK] PromQL query returned one result (12.35)".to_string();

        // Test: Default output, single time series, OK, service object, no performance data.
        assert_eq!(
            process_query_result(&mapping, time_series).unwrap(),
            (expected_output, 0, None)
        );
    }
}
