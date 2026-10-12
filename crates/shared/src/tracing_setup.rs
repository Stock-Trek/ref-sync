use crate::errors::{
    config::RsConfigError,
    error::{RsSharedError, RsSharedResult},
};
use std::env::VarError;
use tracing_subscriber::{EnvFilter, fmt::Subscriber};

pub struct TracingSetup;

impl TracingSetup {
    pub fn setup() -> RsSharedResult<()> {
        let filter = match std::env::var(EnvFilter::DEFAULT_ENV) {
            Ok(directives) => EnvFilter::try_new(&directives).map_err(|error| {
                RsSharedError::Config(RsConfigError::invalid_source(EnvFilter::DEFAULT_ENV, error))
            })?,
            Err(VarError::NotPresent) => EnvFilter::new("info"),
            Err(VarError::NotUnicode(_)) => {
                return Err(RsSharedError::Config(RsConfigError::not_unicode(
                    EnvFilter::DEFAULT_ENV,
                )));
            }
        };
        let subscriber = Subscriber::builder()
            .with_ansi(false)
            .with_env_filter(filter)
            .finish();
        tracing::subscriber::set_global_default(subscriber)?;
        Ok(())
    }
}
