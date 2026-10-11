use crate::{
    dto::{
        event_converter::EventConverter,
        github::github_event_converter::{GitHubEventConverter, GitHubWebhookConfig},
    },
    error::GwhResult,
    event_response::EventResponse,
    sqs_message::GitSource,
};
use serde::{Deserialize, Serialize};
use strum::{Display, EnumIter};

#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, Hash, EnumIter, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GitProvider {
    GitHub,
}

impl From<GitSource> for GitProvider {
    fn from(value: GitSource) -> Self {
        match value {
            GitSource::GitHub { .. } => GitProvider::GitHub,
        }
    }
}

impl EventConverter for GitProvider {
    fn event_response(
        &self,
        config: &GitHubWebhookConfig,
        event: &lambda_http::Request,
    ) -> GwhResult<EventResponse> {
        match self {
            GitProvider::GitHub => GitHubEventConverter.event_response(config, event),
        }
    }
}
