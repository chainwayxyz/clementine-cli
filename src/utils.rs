use crate::{CitreaAddress, errors::BridgeCliError};
use eyre::Context;
use std::str::FromStr;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt};

pub fn parse_citrea_address(citrea_address: &str) -> Result<CitreaAddress, BridgeCliError> {
    Ok(CitreaAddress::from_str(citrea_address).wrap_err("Invalid Citrea address format")?)
}

pub fn initialize_logger(level: Option<LevelFilter>) {
    let filter = match level {
        Some(lvl) => EnvFilter::builder()
            .with_default_directive(lvl.into())
            .from_env_lossy(),
        None => EnvFilter::builder()
            .with_default_directive(LevelFilter::OFF.into())
            .from_env_lossy(),
    };

    let standard_layer = fmt::layer()
        .with_test_writer()
        .with_file(true)
        .with_line_number(true)
        .with_target(true);

    let _ = tracing::subscriber::set_global_default(
        tracing_subscriber::registry()
            .with(standard_layer)
            .with(filter),
    );
}
