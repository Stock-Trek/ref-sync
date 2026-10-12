use crate::{error::RsWebhookResult, event_response::EventResponse};
use lambda_http::Request;

pub trait EventConverter: Send + Sync {
    fn event_response(&self, event: &Request) -> RsWebhookResult<EventResponse>;
}
