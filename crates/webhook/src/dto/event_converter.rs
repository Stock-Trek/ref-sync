use crate::{
    dto::github::github_event_converter::{GITHUB_WEBHOOK_SECRET, GitHubEventConverter},
    error::{RsWebhookError, RsWebhookResult},
    event_response::EventResponse,
    infisical::secrets::Secrets,
};
use lambda_http::Request;
use ref_sync_shared::error::ConfigError;

pub trait EventConverter: Send + Sync {
    fn event_response(&self, event: &Request) -> RsWebhookResult<EventResponse>;
}

pub fn event_converters(secrets: &Secrets) -> RsWebhookResult<Vec<Box<dyn EventConverter>>> {
    let github_secret = secrets.get(GITHUB_WEBHOOK_SECRET)?;
    if github_secret.is_empty() {
        return Err(RsWebhookError::Config(ConfigError::empty(
            GITHUB_WEBHOOK_SECRET,
        )));
    }
    Ok(vec![Box::new(GitHubEventConverter::new(github_secret))])
}
