use metrics::{describe_counter, describe_gauge, describe_histogram};

pub const INVOCATIONS_TOTAL: &str = "invocations_total";
pub const ERRORS_TOTAL: &str = "errors_total";
pub const DURATION_SECONDS: &str = "duration_seconds";
pub const COLD_STARTS_TOTAL: &str = "cold_starts_total";
pub const ACTIVE_REQUESTS: &str = "active_requests";

pub const STATUS_SUCCESS: &str = "success";
pub const STATUS_ERROR: &str = "error";

pub struct Metrics;

impl Metrics {
    pub fn describe() {
        describe_counter!(
            INVOCATIONS_TOTAL,
            "Total number of Lambda invocations, labelled by route and status"
        );
        describe_counter!(
            ERRORS_TOTAL,
            "Total number of Lambda invocation errors, labelled by route and error type"
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
    }
}
