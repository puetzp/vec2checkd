use prometheus_http_query::Scheme;

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
}

impl Default for IcingaClient {
    fn default() -> Self {
        IcingaClient {
            client: reqwest::Client::new(),
            url: String::from("http://127.0.0.1:5665/v1/actions/process-check-result"),
        }
    }
}
