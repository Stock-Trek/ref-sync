use crate::error::{ConfigError, RsSharedError, RsSharedResult};

pub struct EnvVars;

impl EnvVars {
    pub fn csv_env_var(name: &str) -> RsSharedResult<Vec<String>> {
        Ok(Self::env_var(name)?
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .collect())
    }

    pub fn env_var(name: &str) -> RsSharedResult<String> {
        let value = std::env::var(name).map_err(|error| RsSharedError::EnvVar(Box::new(error)))?;
        if value.is_empty() {
            return Err(RsSharedError::Config(ConfigError::empty(name)));
        }
        Ok(value)
    }
}
