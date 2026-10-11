use crate::error::{SharedError, SharedResult};
use std::env::VarError;
use tracing_subscriber::{EnvFilter, fmt::Subscriber};

pub struct TracingSetup;

impl TracingSetup {
    pub fn setup() -> SharedResult<()> {
        let filter = match std::env::var(EnvFilter::DEFAULT_ENV) {
            Ok(directives) => EnvFilter::try_new(&directives).map_err(|error| {
                SharedError::Config(format!("{} is invalid: {error}", EnvFilter::DEFAULT_ENV))
            })?,
            Err(VarError::NotPresent) => EnvFilter::new("info"),
            Err(VarError::NotUnicode(_)) => {
                return Err(SharedError::Config(format!(
                    "{} is not valid UTF-8",
                    EnvFilter::DEFAULT_ENV
                ))
                .into());
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
