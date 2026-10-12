use crate::dto::{
    event_converter::EventConverter,
    github::github_event_converter::{GitHubEventConverter, GitHubWebhookConfig},
};

pub fn event_converters(github: GitHubWebhookConfig) -> Vec<Box<dyn EventConverter>> {
    vec![Box::new(GitHubEventConverter::new(github))]
}
