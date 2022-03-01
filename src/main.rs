mod config;
mod error;
mod helpers;
mod icinga;
mod prometheus;
mod types;
mod util;

use crate::icinga::*;
use crate::types::Mapping;
use crate::util::*;
use gumdrop::Options;
use log::{debug, error, info, warn};
use std::fs::File;
use std::io::Read;
use std::time::Instant;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Options)]
struct AppOptions {
    #[options(help = "print help message", short = "h")]
    help: bool,

    #[options(help = "print version", short = "v")]
    version: bool,

    #[options(help = "path to the configuration file", short = "c")]
    config: String,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), anyhow::Error> {
    let opts = AppOptions::parse_args_default_or_exit();

    if opts.version {
        println!("v{}", VERSION);
        std::process::exit(0);
    }

    env_logger::init();

    info!("Start vec2checkd version {}", &VERSION);

    let config = {
        info!("Parse configuration from '{}'", opts.config);

        if opts.config.is_empty() {
            error!("Path to configuration file cannot be empty. Exit");
            std::process::exit(1);
        }

        let mut file = match File::open(&opts.config) {
            Ok(f) => f,
            Err(e) => {
                error!(
                    "Failed to read configuration file '{}': {:#}",
                    opts.config, e
                );
                std::process::exit(1);
            }
        };

        let mut raw_conf = String::new();
        file.read_to_string(&mut raw_conf)?;

        match config::parse_yaml(&raw_conf) {
            Ok(cfg) => cfg,
            Err(e) => {
                error!(
                    "Failed to parse configuration file '{}': {:#}",
                    opts.config, e
                );
                std::process::exit(1);
            }
        }
    };

    info!("Read mappings between PromQL and Icinga check results from configuration");
    let mut mappings: Vec<Mapping> = match config::parse_mappings(config.clone()) {
        Ok(m) => m,
        Err(e) => {
            error!("Failed to parse mappings from configuration: {:#}", e);
            std::process::exit(1);
        }
    };

    if mappings.is_empty() {
        warn!("No mappings configured. Exit.");
        std::process::exit(0);
    }

    let prom_client = {
        info!("Read Prometheus section from configuration and initialize API client");
        let c = match config::parse_prom_section(&config) {
            Ok(c) => c,
            Err(e) => {
                error!("Failed to parse Icinga section from configuration: {:#}", e);
                std::process::exit(1);
            }
        };
        match prometheus::create_client(c) {
            Ok(clt) => clt,
            Err(e) => {
                error!("Failed to initialize Prometheus API client: {:#}", e);
                std::process::exit(1)
            }
        }
    };

    let icinga_client = {
        info!("Read Icinga section from configuration and initialize API client");
        let c = match config::parse_icinga_section(&config) {
            Ok(c) => c,
            Err(e) => {
                error!("Failed to parse Icinga section from configuration: {:#}", e);
                std::process::exit(1);
            }
        };
        match IcingaClient::new(c) {
            Ok(clt) => clt,
            Err(e) => {
                error!("Failed to initialize Icinga API client: {:#}", e);
                std::process::exit(1)
            }
        }
    };

    info!("Execute every check once regardless of the configured intervals and then enter the periodic check loop");
    let mut initial_check = true;
    loop {
        for mapping in mappings
            .iter_mut()
            .filter(|mapping| compute_delta(mapping).as_secs() <= 1 || initial_check)
        {
            let context = &mapping.name;

            let task_start = Instant::now();

            debug!(
                "{}: update last check time, set to {:?}",
                context, task_start
            );

            mapping.last_apply = task_start;

            match execute_task(prom_client.clone(), icinga_client.clone(), mapping.clone()).await {
                Ok(Ok(())) => {
                    debug!(
                        "'{}': check finished in {} millisecond(s)",
                        context,
                        task_start.elapsed().as_millis()
                    );
                    debug!(
                        "'{}': next check in ~{} second(s)",
                        context,
                        compute_delta(mapping).as_secs()
                    );
                }
                Ok(Err(err)) => {
                    error!("'{}': failed to finish check: {:?}", context, err);
                    debug!(
                        "'{}': retry check in ~{} second(s)",
                        context,
                        compute_delta(mapping).as_secs()
                    );
                }
                Err(err) => {
                    error!("'{}': failed to finish check: {:?}", context, err);
                    debug!(
                        "'{}': retry check in ~{} second(s)",
                        context,
                        compute_delta(mapping).as_secs()
                    );
                }
            }
        }
        initial_check = false;
        let sleep_secs = mappings.iter().map(compute_delta).min().unwrap();
        std::thread::sleep(sleep_secs);
    }
}
