use prometheus_http_query::Client;

pub(crate) fn create_client(config: crate::types::PromConfig) -> Result<Client, anyhow::Error> {
    let mut builder = reqwest::Client::builder();

    if config.proxy.ignore {
        builder = builder.no_proxy();
    } else {
        if let Some(proxy_host) = config.proxy.host {
            builder = builder.proxy(proxy_host);
        }
    }

    let base_client = builder.build()?;

    Ok(Client::from(base_client, &config.host.to_string())?)
}
