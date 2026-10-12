use crate::error::{ConfigError, RefSyncSharedError, SharedResult, TimeoutError};
use std::time::{Duration, SystemTime};

const DEADLINE_BUFFER: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy)]
pub struct Timeouts {
    pub connect: Duration,
    pub operation: Duration,
}

impl Timeouts {
    pub fn from_env(
        name: &str,
        default_connect: Duration,
        default_operation: Duration,
    ) -> SharedResult<Self> {
        Ok(Self {
            connect: Self::duration(&format!("{name}_CONNECT_TIMEOUT"), default_connect)?,
            operation: Self::duration(&format!("{name}_OPERATION_TIMEOUT"), default_operation)?,
        })
    }

    pub fn operation_for(&self, deadline: SystemTime) -> SharedResult<Duration> {
        Ok(self.operation.min(Self::remaining(deadline)?))
    }

    fn remaining(deadline: SystemTime) -> SharedResult<Duration> {
        let remaining = deadline
            .duration_since(SystemTime::now())
            .unwrap_or_default()
            .saturating_sub(DEADLINE_BUFFER);
        if remaining.is_zero() {
            return Err(RefSyncSharedError::Timeout(TimeoutError::DeadlineExceeded));
        }
        Ok(remaining)
    }

    fn duration(key: &str, default: Duration) -> SharedResult<Duration> {
        match std::env::var(key) {
            Ok(value) => {
                let seconds = value
                    .parse::<u64>()
                    .map_err(|error| ConfigError::invalid_source(key, error))?;
                if seconds == 0 {
                    return Err(RefSyncSharedError::Config(ConfigError::invalid(
                        key,
                        "must be greater than 0",
                    )));
                }
                Ok(Duration::from_secs(seconds))
            }
            Err(std::env::VarError::NotPresent) => Ok(default),
            Err(std::env::VarError::NotUnicode(_)) => {
                Err(RefSyncSharedError::Config(ConfigError::not_unicode(key)))
            }
        }
    }
}
