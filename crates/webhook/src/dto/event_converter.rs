use crate::{
    dto::github::github_event_converter::GitHubEventConverter, error::RsWebhookResult,
    event_response::EventResponse, infisical::secrets::Secrets,
};
use lambda_http::Request;
use ref_sync_shared::env_vars::EnvVarConfig;

pub trait EventConverter: Send + Sync {
    fn new(secrets: &Secrets, config: &EnvVarConfig) -> RsWebhookResult<Self>
    where
        Self: Sized;

    fn event_response(&self, event: &Request) -> RsWebhookResult<EventResponse>;
}

pub struct EventConverters;

impl EventConverters {
    pub fn all(
        secrets: &Secrets,
        config: &EnvVarConfig,
    ) -> RsWebhookResult<Vec<Box<dyn EventConverter>>> {
        Ok(vec![Box::new(GitHubEventConverter::new(secrets, config)?)])
    }
}
