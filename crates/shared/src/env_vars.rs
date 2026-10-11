use crate::error::{SharedError, SharedResult};

pub struct EnvVars;

impl EnvVars {
    pub fn csv_env_var(name: &str) -> SharedResult<Vec<String>> {
        Ok(Self::env_var(name)?
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .collect())
    }

    pub fn env_var(name: &str) -> SharedResult<String> {
        std::env::var(name).map_err(|error| SharedError::EnvVar(Box::new(error)))
    }
}
