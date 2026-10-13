use crate::{
    env_vars::EnvVars,
    errors::{
        config::RsConfigError,
        error::{RsSharedError, RsSharedResult},
        timeout::RsTimeoutError,
    },
};
use std::time::{Duration, SystemTime};

const DEADLINE_BUFFER_KEY: &str = "DEADLINE_BUFFER";
const DEFAULT_DEADLINE_BUFFER: Duration = Duration::from_secs(5);

/// An absolute instant by which an operation must finish.
///
/// Wrapping [`SystemTime`] keeps the deadline arithmetic (including the buffer
/// that reserves time for cleanup) in one place and makes the intent of a
/// parameter explicit instead of passing around a bare timestamp.
#[derive(Debug, Clone, Copy)]
pub struct Deadline(SystemTime);

impl Deadline {
    pub fn new(deadline: SystemTime) -> Self {
        Self(deadline)
    }

    /// Time left until the deadline, minus `buffer` reserved for cleanup.
    /// Fails once the deadline has effectively passed.
    pub fn remaining(self, buffer: Duration) -> RsSharedResult<Duration> {
        let remaining = self
            .0
            .duration_since(SystemTime::now())
            .unwrap_or_default()
            .saturating_sub(buffer);
        if remaining.is_zero() {
            return Err(RsSharedError::Timeout(RsTimeoutError::DeadlineExceeded));
        }
        Ok(remaining)
    }

    /// The timeout to use for an operation issued before this deadline: the
    /// configured operation timeout, capped by the time left after the
    /// configured cleanup buffer.
    pub fn operation_timeout(self, timeouts: &Timeouts) -> RsSharedResult<Duration> {
        Ok(timeouts
            .operation
            .min(self.remaining(timeouts.deadline_buffer)?))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Timeouts {
    pub connect: Duration,
    pub operation: Duration,
    pub deadline_buffer: Duration,
}

impl Timeouts {
    pub fn from_env(
        name: &str,
        default_connect: Duration,
        default_operation: Duration,
    ) -> RsSharedResult<Self> {
        Ok(Self {
            connect: Self::duration(&format!("{name}_CONNECT_TIMEOUT"), default_connect)?,
            operation: Self::duration(&format!("{name}_OPERATION_TIMEOUT"), default_operation)?,
            deadline_buffer: Self::duration(DEADLINE_BUFFER_KEY, DEFAULT_DEADLINE_BUFFER)?,
        })
    }

    fn duration(key: &str, default: Duration) -> RsSharedResult<Duration> {
        let Some(value) = EnvVars::optional_env_var(key)? else {
            return Ok(default);
        };
        let seconds = value
            .parse::<u64>()
            .map_err(|error| RsConfigError::invalid_source(key, error))?;
        if seconds == 0 {
            return Err(RsSharedError::Config(RsConfigError::invalid(
                key,
                "must be greater than 0",
            )));
        }
        Ok(Duration::from_secs(seconds))
    }
}
