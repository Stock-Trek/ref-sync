use crate::{
    context::GwhContext,
    dto::event_converter::EventConverter,
    error::{RsWebhookError, RsWebhookResult},
    event_response::EventResponse,
    git_ops::git_remote::GitRemote,
    provider::GitProvider,
    responses::Responses,
};
use http::Response;
use lambda_http::{Body, Request, RequestExt};
use ref_sync_shared::sqs_message::{
    GitSource, SqsDetail, SqsMessage, SqsMultiRepoAction, SqsRefAction, SqsRefType, SqsRepoAction,
};
use std::time::SystemTime;
use strum::IntoEnumIterator;

pub struct WebhookHandler;

impl WebhookHandler {
    pub async fn handle(
        &self,
        context: &GwhContext,
        event: Request,
    ) -> RsWebhookResult<Response<Body>> {
        let deadline = event.lambda_context().deadline();
        let mut ignored = false;
        let mut unauthorized = false;
        for provider in GitProvider::iter() {
            match provider.event_response(&context.webhook, &event) {
                Ok(response) => match response {
                    EventResponse::Accept(message) => {
                        let messages = Self::resolve(context, message, deadline).await?;
                        // TODO send desired state to dynamodb
                        // for message in &messages {
                        //     context.aws.dynamodb.send(message, deadline).await?;
                        // }
                        tracing::info!(
                            count = messages.len(),
                            "Accepted git provider event for processing"
                        );
                        return Responses::accepted();
                    }
                    EventResponse::Pong => return Responses::ok("pong"),
                    EventResponse::Ignore => ignored = true,
                    EventResponse::Unauthorized => unauthorized = true,
                    EventResponse::WrongGitProvider => continue,
                },
                Err(RsWebhookError::Serde(error)) => {
                    tracing::error!("Failed to deserialize signed event: {error}");
                    ignored = true;
                }
                Err(error) => return Err(error),
            }
        }
        if unauthorized {
            Responses::unauthorized()
        } else if ignored {
            Responses::no_content()
        } else {
            Responses::bad_request()
        }
    }

    /// Resolve a provider-agnostic event into one or more fully resolved messages.
    ///
    /// The orchestrator deliberately knows nothing about git providers, so any
    /// provider call (listing repositories, reading a default branch) happens
    /// here, where the provider client lives.
    async fn resolve(
        context: &GwhContext,
        message: SqsMessage,
        deadline: SystemTime,
    ) -> RsWebhookResult<Vec<SqsMessage>> {
        let SqsMessage { source, detail } = message;
        let messages = match detail {
            SqsDetail::AllRepos {
                action: SqsMultiRepoAction::Add,
            } => {
                let repo_ids = GitRemote::from_source(context, &source)?
                    .repo_ids(deadline)
                    .await?;
                Self::ref_messages(context, &source, repo_ids, deadline).await?
            }
            SqsDetail::MultiRepo {
                action: SqsMultiRepoAction::Add,
                repo_ids,
            } => Self::ref_messages(context, &source, repo_ids, deadline).await?,
            SqsDetail::Repo {
                action: SqsRepoAction::Add,
                repo_id,
            } => Self::ref_messages(context, &source, vec![repo_id], deadline).await?,
            detail => vec![SqsMessage { source, detail }],
        };
        Ok(messages)
    }

    async fn ref_messages(
        context: &GwhContext,
        source: &GitSource,
        repo_ids: Vec<String>,
        deadline: SystemTime,
    ) -> RsWebhookResult<Vec<SqsMessage>> {
        let git_remote = GitRemote::from_source(context, source)?;
        let mut messages = Vec::with_capacity(repo_ids.len());
        for repo_id in repo_ids {
            let ref_name = git_remote.repo_default_branch(&repo_id, deadline).await?;
            messages.push(SqsMessage {
                source: source.clone(),
                detail: SqsDetail::Ref {
                    action: SqsRefAction::Create,
                    repo_id,
                    ref_name,
                    ref_type: SqsRefType::Branch,
                },
            });
        }
        Ok(messages)
    }
}
