use crate::errors::{
    config::RsConfigError,
    error::{RsSharedError, RsSharedResult},
};
use std::time::Duration;

pub struct EnvVars;

impl EnvVars {
    pub fn duration(key: &str, default: Duration) -> RsSharedResult<Duration> {
        let Some(value) = EnvVars::optional_env_var(key)? else {
            return Ok(default);
        };
        let seconds = value
            .parse::<u64>()
            .map_err(|error| RsConfigError::invalid_source(key, error))?;
        Ok(Duration::from_secs(seconds))
    }

    pub fn csv_env_var(name: &str) -> RsSharedResult<Vec<String>> {
        match Self::optional_env_var(name)? {
            Some(value) => {
                let csv = Self::split_csv(&value);
                if csv.is_empty() {
                    Err(RsSharedError::Config(RsConfigError::empty(name)))
                } else {
                    Ok(csv)
                }
            }
            None => Err(RsSharedError::Config(RsConfigError::missing(name))),
        }
    }

    pub fn optional_csv_env_var(name: &str) -> RsSharedResult<Vec<String>> {
        match Self::optional_env_var(name)? {
            Some(value) => Ok(Self::split_csv(&value)),
            None => Ok(vec![]),
        }
    }

    pub fn env_var(name: &str) -> RsSharedResult<String> {
        match Self::optional_env_var(name)? {
            Some(value) => Ok(value),
            None => Err(RsSharedError::Config(RsConfigError::missing(name))),
        }
    }

    pub fn optional_env_var(name: &str) -> RsSharedResult<Option<String>> {
        match std::env::var(name) {
            Ok(value) if value.is_empty() => Err(RsSharedError::Config(RsConfigError::empty(name))),
            Ok(value) => Ok(Some(value)),
            Err(std::env::VarError::NotPresent) => Ok(None),
            Err(error) => Err(RsSharedError::EnvVar(Box::new(error))),
        }
    }

    fn split_csv(value: &str) -> Vec<String> {
        value
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .collect()
    }
}
