use crate::{
    dto::github::github_event_converter::GitHubEventConverter, error::RsWebhookResult,
    event_response::EventResponse,
};
use lambda_http::Request;

pub trait EventConverter: Send + Sync {
    fn event_response(&self, event: &Request) -> RsWebhookResult<EventResponse>;
}

pub fn event_converters(github_secret: String) -> Vec<Box<dyn EventConverter>> {
    vec![Box::new(GitHubEventConverter::new(github_secret))]
}
