use crate::error::{SharedError, SharedResult};
use std::{
    future::Future,
    time::{Duration, SystemTime},
};

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

    pub async fn with_deadline<T>(
        &self,
        deadline: SystemTime,
        operation: &str,
        future: impl Future<Output = T>,
    ) -> SharedResult<T> {
        Self::timeout(self.operation_for(deadline)?, operation, future).await
    }

    async fn timeout<T>(
        timeout: Duration,
        operation: &str,
        future: impl Future<Output = T>,
    ) -> SharedResult<T> {
        tokio::time::timeout(timeout, future)
            .await
            .map_err(|_| SharedError::Timeout(format!("{operation} timed out")))
    }

    fn remaining(deadline: SystemTime) -> SharedResult<Duration> {
        let remaining = deadline
            .duration_since(SystemTime::now())
            .unwrap_or_default()
            .saturating_sub(DEADLINE_BUFFER);
        if remaining.is_zero() {
            return Err(SharedError::Timeout(
                "Deadline reached before operation could start".into(),
            ));
        }
        Ok(remaining)
    }

    fn duration(key: &str, default: Duration) -> SharedResult<Duration> {
        match std::env::var(key) {
            Ok(value) => {
                let seconds = value.parse::<u64>().map_err(|_| {
                    SharedError::Config(format!("{key} must be a whole number of seconds"))
                })?;
                if seconds == 0 {
                    return Err(SharedError::Config(format!("{key} must be greater than 0")));
                }
                Ok(Duration::from_secs(seconds))
            }
            Err(std::env::VarError::NotPresent) => Ok(default),
            Err(std::env::VarError::NotUnicode(_)) => {
                Err(SharedError::Config(format!("{key} is not valid UTF-8")))
            }
        }
    }
}
