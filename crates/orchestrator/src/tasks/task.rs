use crate::{
    aws_ops::build_request::BuildRequest,
    context::GwhContext,
    error::{RsOrchestratorError, RsOrchestratorResult},
    tasks::s3_keys::S3Keys,
};
use ref_sync_shared::sqs_message::{
    GitSource, SqsDetail, SqsMessage, SqsMultiRepoAction, SqsRefAction, SqsRefType, SqsRepoAction,
};
use std::time::{Duration, SystemTime};

/// The build agent runs asynchronously in ECS, well after the Lambda invocation
/// that starts it has returned, so the presigned URL must outlive the invocation.
const SIGNED_URL_EXPIRY: Duration = Duration::from_secs(60 * 60);

#[derive(Default)]
struct CommitInfo {
    hash: Option<String>,
    forced: bool,
}

pub enum Task {
    AllReposRemove {
        source: GitSource,
    },
    MultiRepoRemove {
        source: GitSource,
        repo_ids: Vec<String>,
    },
    RepoRemove {
        source: GitSource,
        repo_id: String,
    },
    RefCommit {
        source: GitSource,
        repo_id: String,
        ref_name: String,
        ref_type: SqsRefType,
        commit_hash: Option<String>,
        forced: bool,
    },
    RefDelete {
        source: GitSource,
        repo_id: String,
        ref_name: String,
        ref_type: SqsRefType,
    },
}

impl Task {
    pub async fn handle(
        &self,
        context: &GwhContext,
        deadline: SystemTime,
    ) -> RsOrchestratorResult<()> {
        match self {
            Self::AllReposRemove { source } => {
                let prefix = S3Keys::installation_prefix(source);
                context
                    .aws
                    .s3
                    .delete_objects_with_prefix(&prefix, deadline)
                    .await
            }
            Self::MultiRepoRemove { source, repo_ids } => {
                for repo_id in repo_ids {
                    let repo_prefix = S3Keys::repo_prefix(source, repo_id)?;
                    context
                        .aws
                        .s3
                        .delete_objects_with_prefix(&repo_prefix, deadline)
                        .await?;
                }
                Ok(())
            }
            Self::RepoRemove { source, repo_id } => {
                let repo_prefix = S3Keys::repo_prefix(source, repo_id)?;
                context
                    .aws
                    .s3
                    .delete_objects_with_prefix(&repo_prefix, deadline)
                    .await
            }
            Self::RefCommit {
                source,
                repo_id,
                ref_name,
                ref_type,
                commit_hash,
                forced,
            } => {
                Self::ref_commit(
                    context,
                    source,
                    repo_id,
                    ref_name,
                    *ref_type,
                    CommitInfo {
                        hash: commit_hash.clone(),
                        forced: *forced,
                    },
                    deadline,
                )
                .await
            }
            Self::RefDelete {
                source,
                repo_id,
                ref_name,
                ref_type,
            } => {
                let ref_key = S3Keys::ref_key(source, repo_id, ref_name, *ref_type)?;
                context.aws.s3.delete(&ref_key, deadline).await
            }
        }
    }

    async fn ref_commit(
        context: &GwhContext,
        source: &GitSource,
        repo_id: &str,
        ref_name: &str,
        ref_type: SqsRefType,
        commit: CommitInfo,
        deadline: SystemTime,
    ) -> RsOrchestratorResult<()> {
        let ref_key = S3Keys::ref_key(source, repo_id, ref_name, ref_type)?;
        let signed_s3_put_url = context
            .aws
            .s3
            .signed_put_url(&ref_key, SIGNED_URL_EXPIRY)
            .await?;
        context
            .aws
            .ecs
            .run_build(
                BuildRequest {
                    source: source.clone(),
                    repo_id: repo_id.to_string(),
                    ref_name: ref_name.to_string(),
                    ref_type,
                    commit_hash: commit.hash,
                    forced: commit.forced,
                    signed_s3_put_url,
                },
                deadline,
            )
            .await
    }
}

impl TryFrom<SqsMessage> for Task {
    type Error = RsOrchestratorError;

    fn try_from(value: SqsMessage) -> Result<Self, Self::Error> {
        let SqsMessage { source, detail } = value;
        Ok(match detail {
            SqsDetail::AllRepos {
                action: SqsMultiRepoAction::Remove,
            } => Self::AllReposRemove { source },
            SqsDetail::MultiRepo {
                action: SqsMultiRepoAction::Remove,
                repo_ids,
            } => Self::MultiRepoRemove { source, repo_ids },
            SqsDetail::Repo {
                action: SqsRepoAction::Remove,
                repo_id,
            } => Self::RepoRemove { source, repo_id },
            SqsDetail::Ref {
                action: SqsRefAction::Create,
                repo_id,
                ref_name,
                ref_type,
            } => Self::RefCommit {
                source,
                repo_id,
                ref_name,
                ref_type,
                commit_hash: None,
                forced: false,
            },
            SqsDetail::Ref {
                action: SqsRefAction::Delete,
                repo_id,
                ref_name,
                ref_type,
            } => Self::RefDelete {
                source,
                repo_id,
                ref_name,
                ref_type,
            },
            SqsDetail::Commit {
                repo_id,
                branch_name,
                commit_hash,
                forced,
            } => Self::RefCommit {
                source,
                repo_id,
                ref_name: branch_name,
                ref_type: SqsRefType::Branch,
                commit_hash: Some(commit_hash),
                forced,
            },
            SqsDetail::AllRepos {
                action: SqsMultiRepoAction::Add,
            }
            | SqsDetail::MultiRepo {
                action: SqsMultiRepoAction::Add,
                ..
            }
            | SqsDetail::Repo {
                action: SqsRepoAction::Add,
                ..
            } => {
                return Err(RsOrchestratorError::InvalidMessage(
                    "Add actions must be resolved by the webhook".into(),
                ));
            }
        })
    }
}
