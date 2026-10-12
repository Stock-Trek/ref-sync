use crate::{
    aws_ops::{
        build_request::BuildRequest,
        tombstones::{DetectedVia, Reason, RefType, ScopeType, Tombstone, TombstoneContext},
    },
    context::RsOrchestratorContext,
    error::RsOrchestratorResult,
    tasks::s3_keys::S3Keys,
};
use ref_sync_shared::{
    git_ops::git_remote::GitRemote,
    request_context::RequestContext,
    sqs_message::{
        GitSource, SqsDetail, SqsMessage, SqsMultiRepoAction, SqsRefAction, SqsRefType,
        SqsRepoAction,
    },
};
use std::time::Duration;

const SIGNED_URL_EXPIRY: Duration = Duration::from_secs(60 * 60);

#[derive(Default)]
struct CommitInfo {
    hash: Option<String>,
    forced: bool,
}

pub enum Task {
    AllReposAdd {
        source: GitSource,
    },
    AllReposRemove {
        source: GitSource,
    },
    MultiRepoAdd {
        source: GitSource,
        repo_ids: Vec<String>,
    },
    MultiRepoRemove {
        source: GitSource,
        repo_ids: Vec<String>,
    },
    RepoAdd {
        source: GitSource,
        repo_id: String,
    },
    RepoRemove {
        source: GitSource,
        repo_id: String,
    },
    RepoTransfer {
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
        request_context: RequestContext<&RsOrchestratorContext>,
    ) -> RsOrchestratorResult<()> {
        match self {
            Self::AllReposAdd { source } => {
                let git_remote = GitRemote::from_source(&request_context.context.github, source)?;
                let repo_ids = git_remote.repo_ids(request_context.deadline).await?;
                Self::multi_repo_task(request_context, source, SqsRepoAction::Add, &repo_ids).await
            }
            Self::AllReposRemove { source } => {
                let ctx = TombstoneContext::new(source, Reason::ScopeDeleted, DetectedVia::Webhook);
                let scope_id = match source {
                    GitSource::GitHub {
                        installation_id, ..
                    } => installation_id.to_string(),
                };
                let tombstone = Tombstone::scope(&ctx, ScopeType::Installation, scope_id);
                Ok(request_context
                    .context
                    .aws
                    .dynamodb
                    .add_tombstone(&tombstone, request_context.deadline)
                    .await?)
            }
            Self::MultiRepoAdd { source, repo_ids } => {
                Self::multi_repo_task(request_context, source, SqsRepoAction::Add, repo_ids).await
            }
            Self::MultiRepoRemove { source, repo_ids } => {
                Self::multi_repo_task(request_context, source, SqsRepoAction::Remove, repo_ids)
                    .await
            }
            Self::RepoAdd { source, repo_id } => {
                Self::repo_add(request_context, source, repo_id).await
            }
            Self::RepoRemove { source, repo_id } => {
                let ctx = TombstoneContext::new(source, Reason::RepoDeleted, DetectedVia::Webhook);
                let tombstone = Tombstone::repository(&ctx, repo_id.clone());
                Ok(request_context
                    .context
                    .aws
                    .dynamodb
                    .add_tombstone(&tombstone, request_context.deadline)
                    .await?)
            }
            Self::RepoTransfer { source, repo_id } => {
                let ctx =
                    TombstoneContext::new(source, Reason::RepoTransferred, DetectedVia::Webhook);
                let tombstone = Tombstone::repository(&ctx, repo_id.clone());
                request_context
                    .context
                    .aws
                    .dynamodb
                    .add_tombstone(&tombstone, request_context.deadline)
                    .await?;
                Self::repo_add(request_context, source, repo_id).await
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
                    request_context,
                    source,
                    repo_id,
                    ref_name,
                    *ref_type,
                    CommitInfo {
                        hash: commit_hash.clone(),
                        forced: *forced,
                    },
                )
                .await
            }
            Self::RefDelete {
                source,
                repo_id,
                ref_name,
                ref_type,
            } => {
                let ctx = TombstoneContext::new(source, Reason::RefDeleted, DetectedVia::Webhook);
                let tombstone_ref_type = match ref_type {
                    SqsRefType::Branch => RefType::Branch,
                    SqsRefType::Tag => RefType::Tag,
                };
                let tombstone = Tombstone::reference(
                    &ctx,
                    repo_id.clone(),
                    ref_name.clone(),
                    tombstone_ref_type,
                );
                Ok(request_context
                    .context
                    .aws
                    .dynamodb
                    .add_tombstone(&tombstone, request_context.deadline)
                    .await?)
            }
        }
    }

    async fn multi_repo_task(
        request_context: RequestContext<&RsOrchestratorContext>,
        source: &GitSource,
        action: SqsRepoAction,
        repo_ids: &[String],
    ) -> RsOrchestratorResult<()> {
        let messages = repo_ids
            .iter()
            .map(|repo_id| SqsMessage {
                source: source.clone(),
                detail: SqsDetail::Repo {
                    action,
                    repo_id: repo_id.clone(),
                },
            })
            .collect::<Vec<_>>();
        Ok(request_context
            .context
            .aws
            .sqs
            .send_batch(&messages, request_context.deadline)
            .await?)
    }

    async fn repo_add(
        request_context: RequestContext<&RsOrchestratorContext>,
        source: &GitSource,
        repo_id: &str,
    ) -> RsOrchestratorResult<()> {
        let git_remote = GitRemote::from_source(&request_context.context.github, source)?;
        let default_branch = git_remote
            .repo_default_branch(repo_id, request_context.deadline)
            .await?;
        Self::ref_commit(
            request_context,
            source,
            repo_id,
            &default_branch,
            SqsRefType::Branch,
            CommitInfo::default(),
        )
        .await
    }

    async fn ref_commit(
        request_context: RequestContext<&RsOrchestratorContext>,
        source: &GitSource,
        repo_id: &str,
        ref_name: &str,
        ref_type: SqsRefType,
        commit: CommitInfo,
    ) -> RsOrchestratorResult<()> {
        let git_remote = GitRemote::from_source(&request_context.context.github, source)?;
        let ref_key = S3Keys::ref_key(&git_remote, repo_id, ref_name, ref_type)?;
        let signed_s3_put_url = request_context
            .context
            .aws
            .s3
            .signed_put_url(&ref_key, SIGNED_URL_EXPIRY)
            .await?;
        request_context
            .context
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
                request_context.deadline,
            )
            .await
    }
}

impl From<SqsMessage> for Task {
    fn from(value: SqsMessage) -> Self {
        let SqsMessage { source, detail } = value;
        match detail {
            SqsDetail::AllRepos { action } => match action {
                SqsMultiRepoAction::Add => Self::AllReposAdd { source },
                SqsMultiRepoAction::Remove => Self::AllReposRemove { source },
            },
            SqsDetail::MultiRepo { action, repo_ids } => match action {
                SqsMultiRepoAction::Add => Self::MultiRepoAdd { source, repo_ids },
                SqsMultiRepoAction::Remove => Self::MultiRepoRemove { source, repo_ids },
            },
            SqsDetail::Repo { action, repo_id } => match action {
                SqsRepoAction::Add => Self::RepoAdd { source, repo_id },
                SqsRepoAction::Remove => Self::RepoRemove { source, repo_id },
                SqsRepoAction::Transfer => Self::RepoTransfer { source, repo_id },
            },
            SqsDetail::Ref {
                action,
                repo_id,
                ref_name,
                ref_type,
            } => match action {
                SqsRefAction::Create => Self::RefCommit {
                    source,
                    repo_id,
                    ref_name,
                    ref_type,
                    commit_hash: None,
                    forced: false,
                },
                SqsRefAction::Delete => Self::RefDelete {
                    source,
                    repo_id,
                    ref_name,
                    ref_type,
                },
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
        }
    }
}
