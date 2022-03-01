use prometheus_http_query::Client;

pub(crate) fn create_client(config: crate::types::PromConfig) -> Result<Client, anyhow::Error> {
    let mut builder = reqwest::Client::builder();

    if config.proxy.ignore {
        builder = builder.no_proxy();
    } else {
        if let Some(http_proxy) = config.proxy.http {
            builder = builder.proxy(http_proxy);
        }

        if let Some(https_proxy) = config.proxy.https {
            builder = builder.proxy(https_proxy);
        }
    }

    let base_client = builder.build()?;

    Ok(Client::from(base_client, &config.host)?)
}
