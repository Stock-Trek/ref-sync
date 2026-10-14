use crate::errors::{
    error::{RsSharedError, RsSharedResult},
    timeout::RsTimeoutError,
};
use std::time::{Duration, Instant, SystemTime};

#[derive(Debug, Clone, Copy)]
pub struct Deadline {
    at: Instant,
    buffer: Duration,
}

impl Deadline {
    pub fn new(deadline: SystemTime, buffer: Duration) -> Self {
        let remaining = deadline
            .duration_since(SystemTime::now())
            .unwrap_or_default();
        Self {
            at: Instant::now() + remaining,
            buffer,
        }
    }

    pub fn remaining(self) -> RsSharedResult<Duration> {
        let remaining = self
            .at
            .saturating_duration_since(Instant::now())
            .saturating_sub(self.buffer);
        if remaining.is_zero() {
            return Err(RsSharedError::Timeout(RsTimeoutError::DeadlineExceeded));
        }
        Ok(remaining)
    }
}
