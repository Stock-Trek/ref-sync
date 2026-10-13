use crate::{
    env_vars::EnvVars,
    errors::{
        error::{RsSharedError, RsSharedResult},
        timeout::RsTimeoutError,
    },
};
use std::{
    sync::OnceLock,
    time::{Duration, SystemTime},
};

const DEADLINE_BUFFER_KEY: &str = "DEADLINE_BUFFER";
const DEFAULT_DEADLINE_BUFFER: Duration = Duration::from_secs(5);
static DEADLINE_BUFFER: OnceLock<Duration> = OnceLock::new();

#[derive(Debug, Clone, Copy)]
pub struct Deadline(SystemTime);

impl Deadline {
    pub fn new(deadline: SystemTime) -> Self {
        Self(deadline)
    }

    pub fn remaining(self) -> RsSharedResult<Duration> {
        let remaining = self
            .0
            .duration_since(SystemTime::now())
            .unwrap_or_default()
            .saturating_sub(Self::deadline_buffer()?);
        if remaining.is_zero() {
            return Err(RsSharedError::Timeout(RsTimeoutError::DeadlineExceeded));
        }
        Ok(remaining)
    }

    fn deadline_buffer() -> RsSharedResult<Duration> {
        if let Some(buffer) = DEADLINE_BUFFER.get() {
            return Ok(*buffer);
        }
        let buffer = EnvVars::duration(DEADLINE_BUFFER_KEY, DEFAULT_DEADLINE_BUFFER)?;
        Ok(*DEADLINE_BUFFER.get_or_init(|| buffer))
    }
}
