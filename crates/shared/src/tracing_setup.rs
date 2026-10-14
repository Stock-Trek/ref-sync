use crate::{
    env_vars::EnvVarConfig,
    errors::{
        config::RsConfigError,
        error::{RsSharedError, RsSharedResult},
    },
};
use tracing_subscriber::{EnvFilter, fmt::Subscriber};

pub struct TracingSetup;

impl TracingSetup {
    pub fn setup(config: &EnvVarConfig) -> RsSharedResult<()> {
        let filter = match config.log_filter()? {
            Some(directives) => EnvFilter::try_new(&directives).map_err(|error| {
                RsSharedError::Config(RsConfigError::invalid_source(EnvFilter::DEFAULT_ENV, error))
            })?,
            None => EnvFilter::new("info"),
        };
        let subscriber = Subscriber::builder()
            .with_ansi(false)
            .with_env_filter(filter)
            .finish();
        tracing::subscriber::set_global_default(subscriber)?;
        Ok(())
    }
}
