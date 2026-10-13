use crate::{deadline::Deadline, env_vars::EnvVars, errors::error::RsSharedResult};
use std::time::Duration;

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
    ) -> RsSharedResult<Self> {
        Ok(Self {
            connect: EnvVars::duration(&format!("{name}_CONNECT_TIMEOUT"), default_connect)?,
            operation: EnvVars::duration(&format!("{name}_OPERATION_TIMEOUT"), default_operation)?,
        })
    }

    pub fn operation_timeout(self, deadline: Deadline) -> RsSharedResult<Duration> {
        Ok(self.operation.min(deadline.remaining()?))
    }
}
