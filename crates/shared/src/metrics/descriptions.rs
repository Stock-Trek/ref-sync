use metrics::{describe_counter, describe_gauge, describe_histogram};

pub const INVOCATIONS_TOTAL: &str = "invocations_total";
pub const ERRORS_TOTAL: &str = "errors_total";
pub const IGNORED_EVENTS_TOTAL: &str = "ignored_events_total";
pub const DUPLICATE_EVENTS_TOTAL: &str = "duplicate_events_total";
pub const DURATION_SECONDS: &str = "duration_seconds";
pub const COLD_STARTS_TOTAL: &str = "cold_starts_total";
pub const ACTIVE_REQUESTS: &str = "active_requests";
pub const SECRET_CACHE_MISSES_TOTAL: &str = "secret_cache_misses_total";
pub const SECRET_LOAD_ATTEMPTS_TOTAL: &str = "secret_load_attempts_total";
pub const SECRET_LOAD_FAILURES_TOTAL: &str = "secret_load_failures_total";
pub const SECRET_LOAD_BACKOFF_HITS_TOTAL: &str = "secret_load_backoff_hits_total";
pub const SECRET_LOAD_IN_FLIGHT_WAITS_TOTAL: &str = "secret_load_in_flight_waits_total";
pub const SECRET_LOAD_SUCCESSES_TOTAL: &str = "secret_load_successes_total";
pub const MESSAGES_SENT_TOTAL: &str = "messages_sent_total";
pub const MESSAGES_FAILED_TOTAL: &str = "messages_failed_total";

pub const STATUS_SUCCESS: &str = "success";
pub const STATUS_ERROR: &str = "error";

pub struct DescriptionsMetrics;

impl DescriptionsMetrics {
    pub fn describe() {
        describe_counter!(
            INVOCATIONS_TOTAL,
            "Total number of Lambda invocations, labelled by route and status"
        );
        describe_counter!(
            ERRORS_TOTAL,
            "Total number of Lambda invocation errors, labelled by route and error type"
        );
        describe_counter!(
            IGNORED_EVENTS_TOTAL,
            "Total number of webhook events ignored, labelled by event and action"
        );
        describe_counter!(
            DUPLICATE_EVENTS_TOTAL,
            "Total number of webhook deliveries dropped as duplicates"
        );
        describe_histogram!(
            DURATION_SECONDS,
            "Duration of Lambda invocations in seconds, labelled by route"
        );
        describe_counter!(COLD_STARTS_TOTAL, "Total number of Lambda cold starts");
        describe_gauge!(
            ACTIVE_REQUESTS,
            "Number of Lambda requests currently being handled"
        );
        describe_counter!(
            SECRET_CACHE_MISSES_TOTAL,
            "Total number of secret cache misses"
        );
        describe_counter!(
            SECRET_LOAD_ATTEMPTS_TOTAL,
            "Total number of secret load attempts"
        );
        describe_counter!(
            SECRET_LOAD_FAILURES_TOTAL,
            "Total number of failed secret loads, labelled by error type"
        );
        describe_counter!(
            SECRET_LOAD_BACKOFF_HITS_TOTAL,
            "Total number of secret load attempts skipped due to failure backoff"
        );
        describe_counter!(
            SECRET_LOAD_IN_FLIGHT_WAITS_TOTAL,
            "Total number of requests that waited for an in-flight secret load"
        );
        describe_counter!(
            SECRET_LOAD_SUCCESSES_TOTAL,
            "Total number of successful secret loads"
        );
        describe_counter!(
            MESSAGES_SENT_TOTAL,
            "Total number of individual SQS messages sent successfully"
        );
        describe_counter!(
            MESSAGES_FAILED_TOTAL,
            "Total number of individual SQS messages that failed to send"
        );
    }
}
