use crate::{deadline::Deadline, env_vars::EnvVarConfig, errors::error::RsSharedResult};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NonZeroDuration(Duration);

impl NonZeroDuration {
    pub const fn new(duration: Duration) -> Option<Self> {
        if duration.is_zero() {
            None
        } else {
            Some(Self(duration))
        }
    }

    pub const fn get(self) -> Duration {
        self.0
    }
}

impl From<NonZeroDuration> for Duration {
    fn from(value: NonZeroDuration) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Timeout {
    Disabled,
    Bounded(NonZeroDuration),
}

impl Timeout {
    pub fn duration(self) -> Option<Duration> {
        match self {
            Self::Disabled => None,
            Self::Bounded(duration) => Some(duration.get()),
        }
    }
}

impl From<Timeout> for Option<Duration> {
    fn from(timeout: Timeout) -> Self {
        timeout.duration()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Timeouts {
    pub connect: Timeout,
    pub operation: Timeout,
}

impl Timeouts {
    pub fn from_env(
        config: &EnvVarConfig,
        name: &str,
        default_connect: Duration,
        default_operation: Duration,
    ) -> RsSharedResult<Self> {
        Ok(Self {
            connect: Self::timeout(config, &format!("{name}_CONNECT_TIMEOUT"), default_connect)?,
            operation: Self::timeout(
                config,
                &format!("{name}_OPERATION_TIMEOUT"),
                default_operation,
            )?,
        })
    }

    pub fn operation_timeout(self, deadline: Deadline) -> RsSharedResult<Duration> {
        let remaining = deadline.remaining()?;
        Ok(match self.operation {
            Timeout::Disabled => remaining,
            Timeout::Bounded(operation) => operation.get().min(remaining),
        })
    }

    fn timeout(config: &EnvVarConfig, key: &str, default: Duration) -> RsSharedResult<Timeout> {
        let timeout = config.duration(key, default)?;
        Ok(NonZeroDuration::new(timeout).map_or(Timeout::Disabled, Timeout::Bounded))
    }
}
