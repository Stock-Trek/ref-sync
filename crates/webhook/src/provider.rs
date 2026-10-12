use crate::{
    dto::{
        event_converter::EventConverter,
        github::github_event_converter::{GitHubEventConverter, GitHubWebhookConfig},
    },
    error::RsWebhookResult,
    event_response::EventResponse,
};
pub use ref_sync_shared::provider::GitProvider;

impl EventConverter for GitProvider {
    fn event_response(
        &self,
        config: &GitHubWebhookConfig,
        event: &lambda_http::Request,
    ) -> RsWebhookResult<EventResponse> {
        match self {
            GitProvider::GitHub => GitHubEventConverter.event_response(config, event),
        }
    }
}
