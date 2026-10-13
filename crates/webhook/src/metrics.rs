use http::StatusCode;
use metrics::{counter, gauge, histogram};
use ref_sync_shared::metrics::{
    ACTIVE_REQUESTS, DURATION_SECONDS, ERRORS_TOTAL, INVOCATIONS_TOTAL,
};
use std::time::Instant;

pub const WEBHOOK_ROUTE: &str = "webhook";

pub struct RequestMetrics {
    route: &'static str,
    started_at: Instant,
}

impl RequestMetrics {
    pub fn start(route: &'static str) -> Self {
        gauge!(ACTIVE_REQUESTS).increment(1.0);
        Self {
            route,
            started_at: Instant::now(),
        }
    }

    pub fn record_success(&self, status: StatusCode) {
        self.record_duration();
        counter!(
            INVOCATIONS_TOTAL,
            "route" => self.route,
            "status" => status.as_u16().to_string(),
        )
        .increment(1);
    }

    pub fn record_error(&self, error_type: &'static str) {
        self.record_duration();
        counter!(
            INVOCATIONS_TOTAL,
            "route" => self.route,
            "status" => "error",
        )
        .increment(1);
        counter!(
            ERRORS_TOTAL,
            "route" => self.route,
            "error_type" => error_type,
        )
        .increment(1);
    }

    fn record_duration(&self) {
        histogram!(DURATION_SECONDS, "route" => self.route)
            .record(self.started_at.elapsed().as_secs_f64());
    }
}

impl Drop for RequestMetrics {
    fn drop(&mut self) {
        gauge!(ACTIVE_REQUESTS).decrement(1.0);
    }
}
