use crate::types::{Mapping, ThresholdPair};
use prometheus_http_query::Scheme;
use serde::Serialize;

#[derive(Clone)]
pub(crate) struct IcingaClient {
    client: reqwest::Client,
    url: String,
}

impl IcingaClient {
    pub fn new(scheme: Scheme, host: &str, port: u16) -> Self {
        IcingaClient {
            client: reqwest::Client::new(),
            url: format!(
                "{}://{}:{}/v1/actions/process-check-result",
                scheme, host, port
            ),
        }
    }

    pub async fn send(&self, payload: &IcingaPayload) -> Result<(), anyhow::Error> {
        let body = serde_json::to_string(payload)?;
        self.client
            .post(&self.url)
            .body(body)
            .header("Accept", "application/json")
            .send()
            .await?;
        Ok(())
    }
}

impl Default for IcingaClient {
    fn default() -> Self {
        IcingaClient {
            client: reqwest::Client::new(),
            url: String::from("http://127.0.0.1:5665/v1/actions/process-check-result"),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct IcingaPayload {
    exit_status: u8,
    plugin_output: String,
    filter: String,
}

pub(crate) fn determine_exit_status(thresholds: &ThresholdPair, value: f64) -> u8 {
    if let Some(critical) = &thresholds.critical {
        if critical.check(value) {
            return 2;
        }
    }

    if let Some(warning) = &thresholds.warning {
        if warning.check(value) {
            return 1;
        }
    }

    0
}

pub(crate) fn build_payload(mapping: &Mapping, value: f64, exit_status: u8) -> IcingaPayload {
    let filter = format!(
        "host.name==\"{}\" && service.name==\"{}\"",
        mapping.host, mapping.service
    );

    let plugin_output = match exit_status {
        2 => format!("[CRITICAL] {} is {}", mapping.name, value),
        1 => format!("[WARNING] {} is {}", mapping.name, value),
        0 => format!("[OK] {} is {}", mapping.name, value),
        _ => unreachable!(),
    };

    IcingaPayload {
        exit_status,
        plugin_output,
        filter,
    }
}
