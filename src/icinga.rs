use crate::types::*;
use log::debug;
use reqwest::{Certificate, Identity};
use serde::Serialize;
use std::fs::File;
use std::io::Read;

#[derive(Clone)]
pub(crate) struct IcingaClient {
    client: reqwest::Client,
    url: String,
    basic_auth: Option<IcingaBasicAuth>,
}

impl IcingaClient {
    pub fn new(config: &IcingaConfig) -> Result<Self, anyhow::Error> {
        let mut builder = match &config.authentication {
            IcingaAuth::Basic(_) => reqwest::Client::builder(),
            IcingaAuth::X509(auth) => {
                let identity = {
                    let mut buf = Vec::new();
                    debug!("Read client certificate (PEM) from {:?}", auth.client_cert);
                    File::open(&auth.client_cert)?.read_to_end(&mut buf)?;
                    debug!("Read client key (PEM) from {:?}", auth.client_key);
                    File::open(&auth.client_key)?.read_to_end(&mut buf)?;
                    Identity::from_pem(&buf)?
                };

                reqwest::Client::builder().identity(identity)
            }
        };

        builder = builder.min_tls_version(reqwest::tls::Version::TLS_1_2);

        if let Some(cert) = &config.ca_cert {
            let cert_obj = {
                let mut buf = Vec::new();
                debug!("Read CA certificate (PEM) from {:?}", cert);
                File::open(&cert)?.read_to_end(&mut buf)?;
                Certificate::from_pem(&buf)?
            };

            builder = builder.add_root_certificate(cert_obj);
        };

        let client = builder.build()?;

        let url = format!(
            "{}://{}:{}/v1/actions/process-check-result",
            config.scheme, config.host, config.port
        );

        debug!("Set API URL to send passive check results to {}", url);

        let basic_auth = match &config.authentication {
            IcingaAuth::Basic(auth) => Some(auth.clone()),
            IcingaAuth::X509(_) => None,
        };

        Ok(IcingaClient {
            client,
            url,
            basic_auth,
        })
    }

    pub async fn send(&self, payload: &IcingaPayload) -> Result<(), anyhow::Error> {
        let mut builder = self
            .client
            .request(reqwest::Method::POST, &self.url)
            .json(payload)
            .header("Accept", "application/json");

        if let Some(auth) = &self.basic_auth {
            builder = builder.basic_auth(&auth.username, Some(&auth.password));
        }

        let request = builder.build()?;

        self.client.execute(request).await?.error_for_status()?;

        Ok(())
    }
}

impl Default for IcingaClient {
    fn default() -> Self {
        IcingaClient {
            client: reqwest::Client::new(),
            url: String::from("http://127.0.0.1:5665/v1/actions/process-check-result"),
            basic_auth: None,
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
