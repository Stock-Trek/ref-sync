use http::StatusCode;
use metrics::{counter, gauge, histogram};
use ref_sync_shared::metrics::{
    ACTIVE_REQUESTS, DURATION_SECONDS, ERRORS_TOTAL, IGNORED_EVENTS_TOTAL, INVOCATIONS_TOTAL,
    SECRET_CACHE_MISSES_TOTAL, SECRET_LOAD_ATTEMPTS_TOTAL, SECRET_LOAD_BACKOFF_HITS_TOTAL,
    SECRET_LOAD_FAILURES_TOTAL, SECRET_LOAD_IN_FLIGHT_WAITS_TOTAL, SECRET_LOAD_SUCCESSES_TOTAL,
    STATUS_ERROR, STATUS_SUCCESS,
};
use std::time::Instant;

pub const WEBHOOK_ROUTE: &str = "webhook";
pub const ERROR_TYPE_HTTP: &str = "http";
pub const UNKNOWN_ACTION: &str = "unknown";
pub const NO_ACTION: &str = "none";

pub fn record_ignored_event(event: &str, action: &str) {
    counter!(
        IGNORED_EVENTS_TOTAL,
        "event" => event.to_owned(),
        "action" => action.to_owned(),
    )
    .increment(1);
}

pub fn record_secret_cache_miss() {
    counter!(SECRET_CACHE_MISSES_TOTAL).increment(1);
}

pub fn record_secret_load_attempt() {
    counter!(SECRET_LOAD_ATTEMPTS_TOTAL).increment(1);
}

pub fn record_secret_load_failure(error_type: &'static str) {
    counter!(SECRET_LOAD_FAILURES_TOTAL, "error_type" => error_type).increment(1);
}

pub fn record_secret_load_backoff_hit() {
    counter!(SECRET_LOAD_BACKOFF_HITS_TOTAL).increment(1);
}

pub fn record_secret_load_in_flight_wait() {
    counter!(SECRET_LOAD_IN_FLIGHT_WAITS_TOTAL).increment(1);
}

pub fn record_secret_load_success() {
    counter!(SECRET_LOAD_SUCCESSES_TOTAL).increment(1);
}

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
            "status" => STATUS_SUCCESS,
            "http_status" => status.as_u16().to_string(),
        )
        .increment(1);
    }

    pub fn record_error(&self, error_type: &'static str, status: StatusCode) {
        self.record_duration();
        counter!(
            INVOCATIONS_TOTAL,
            "route" => self.route,
            "status" => STATUS_ERROR,
            "http_status" => status.as_u16().to_string(),
        )
        .increment(1);
        counter!(
            ERRORS_TOTAL,
            "route" => self.route,
            "error_type" => error_type,
            "http_status" => status.as_u16().to_string(),
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
