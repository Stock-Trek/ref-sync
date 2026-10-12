use crate::error::{ConfigError, RsSharedError, RsSharedResult};

pub struct EnvVars;

impl EnvVars {
    pub fn csv_env_var(name: &str) -> RsSharedResult<Vec<String>> {
        let csv = Self::optional_csv_env_var(name)?;
        if csv.is_empty() {
            Err(RsSharedError::Config(ConfigError::empty(name)))
        } else {
            Ok(csv)
        }
    }

    pub fn optional_csv_env_var(name: &str) -> RsSharedResult<Vec<String>> {
        if let Some(value) = Self::optional_env_var(name)? {
            Ok(value
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .collect())
        } else {
            Ok(vec![])
        }
    }

    pub fn env_var(name: &str) -> RsSharedResult<String> {
        if let Some(value) = Self::optional_env_var(name)? {
            Ok(value)
        } else {
            Err(RsSharedError::Config(ConfigError::empty(name)))
        }
    }

    pub fn optional_env_var(name: &str) -> RsSharedResult<Option<String>> {
        match std::env::var(name) {
            Ok(value) => Ok(Some(value)),
            Err(std::env::VarError::NotPresent) => Ok(None),
            Err(error) => Err(RsSharedError::EnvVar(Box::new(error))),
        }
    }
}
