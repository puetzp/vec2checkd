use crate::types::IcingaConfig;
use crate::types::{Mapping, ThresholdPair};
use reqwest::{Certificate, Identity};
use serde::Serialize;
use std::fs::File;
use std::io::Read;

#[derive(Clone)]
pub(crate) struct IcingaClient {
    client: reqwest::Client,
    url: String,
}

impl IcingaClient {
    pub fn new(config: &IcingaConfig) -> Result<Self, anyhow::Error> {
        let ca_cert = {
            let mut buf = Vec::new();
            File::open(&config.ca_cert)?.read_to_end(&mut buf)?;
            Certificate::from_pem(&buf)?
        };

        let identity = {
            let mut buf = Vec::new();
            File::open(&config.client_cert)?.read_to_end(&mut buf)?;
            File::open(&config.client_key)?.read_to_end(&mut buf)?;
            Identity::from_pem(&buf)?
        };

        let client = reqwest::Client::builder()
            .identity(identity)
            .add_root_certificate(ca_cert)
            .build()?;

        Ok(IcingaClient {
            client,
            url: format!(
                "{}://{}:{}/v1/actions/process-check-result",
                config.scheme, config.host, config.port
            ),
        })
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
