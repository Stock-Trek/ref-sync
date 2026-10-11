use crate::error::{RefSyncSharedError, SharedResult};
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
            return Err(RefSyncSharedError::Timeout(
                "Deadline reached before operation could start".into(),
            ));
        }
        Ok(remaining)
    }

    fn duration(key: &str, default: Duration) -> SharedResult<Duration> {
        match std::env::var(key) {
            Ok(value) => {
                let seconds = value.parse::<u64>().map_err(|_| {
                    RefSyncSharedError::Config(format!("{key} must be a whole number of seconds"))
                })?;
                if seconds == 0 {
                    return Err(RefSyncSharedError::Config(format!(
                        "{key} must be greater than 0"
                    )));
                }
                Ok(Duration::from_secs(seconds))
            }
            Err(std::env::VarError::NotPresent) => Ok(default),
            Err(std::env::VarError::NotUnicode(_)) => Err(RefSyncSharedError::Config(format!(
                "{key} is not valid UTF-8"
            ))),
        }
    }
}
