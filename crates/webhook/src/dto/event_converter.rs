use crate::{
    dto::github::github_event_converter::GitHubEventConverter, error::RsWebhookResult,
    event_response::EventResponse, infisical::secrets::Secrets,
};
use lambda_http::Request;

pub trait EventConverter: Send + Sync {
    fn new(secrets: &Secrets) -> RsWebhookResult<Self>
    where
        Self: Sized;

    fn event_response(&self, event: &Request) -> RsWebhookResult<EventResponse>;
}

pub fn event_converters(secrets: &Secrets) -> RsWebhookResult<Vec<Box<dyn EventConverter>>> {
    Ok(vec![Box::new(GitHubEventConverter::new(secrets)?)])
}
