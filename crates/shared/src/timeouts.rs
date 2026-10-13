use crate::{deadline::Deadline, env_vars::EnvVars, errors::error::RsSharedResult};
use std::time::Duration;

#[derive(Debug, Clone, Copy)]
pub struct Timeouts {
    pub connect: Option<Duration>,
    pub operation: Option<Duration>,
}

impl Timeouts {
    pub fn from_env(
        name: &str,
        default_connect: Duration,
        default_operation: Duration,
    ) -> RsSharedResult<Self> {
        Ok(Self {
            connect: Self::timeout(&format!("{name}_CONNECT_TIMEOUT"), default_connect)?,
            operation: Self::timeout(&format!("{name}_OPERATION_TIMEOUT"), default_operation)?,
        })
    }

    pub fn operation_timeout(self, deadline: Deadline) -> RsSharedResult<Duration> {
        let remaining = deadline.remaining()?;
        Ok(self
            .operation
            .map_or(remaining, |operation| operation.min(remaining)))
    }

    fn timeout(key: &str, default: Duration) -> RsSharedResult<Option<Duration>> {
        let timeout = EnvVars::duration(key, default)?;
        Ok((!timeout.is_zero()).then_some(timeout))
    }
}
