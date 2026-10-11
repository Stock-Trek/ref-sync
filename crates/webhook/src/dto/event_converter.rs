use crate::{
    dto::github::github_event_converter::GitHubWebhookConfig, error::GwhResult,
    event_response::EventResponse,
};
use lambda_http::Request;

pub trait EventConverter {
    fn event_response(
        &self,
        config: &GitHubWebhookConfig,
        event: &Request,
    ) -> GwhResult<EventResponse>;
}
