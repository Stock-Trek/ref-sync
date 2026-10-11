use crate::{
    aws_ops::build_request::BuildRequest, context::GwhContext, error::RsOrchestratorResult,
    git_ops::git_remote::GitRemote, tasks::s3_keys::S3Keys,
};
use ref_sync_shared::sqs_message::{
    GitSource, SqsDetail, SqsMessage, SqsMultiRepoAction, SqsRefAction, SqsRefType, SqsRepoAction,
};
use std::{
    collections::BTreeSet,
    time::{Duration, SystemTime},
};

/// The build agent runs asynchronously in ECS, well after the Lambda invocation
/// that starts it has returned, so the presigned URL must outlive the invocation.
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
            Self::AllReposAdd { source } => {
                let git_remote = GitRemote::from_source(context, source)?;
                let repo_ids = git_remote.repo_ids(deadline).await?;
                Self::multi_repo_task(context, source, SqsRepoAction::Add, &repo_ids, deadline)
                    .await
            }
            Self::AllReposRemove { source } => {
                let git_remote = GitRemote::from_source(context, source)?;
                let prefix = S3Keys::installation_prefix(&git_remote);
                let keys = context
                    .aws
                    .s3
                    .list_keys_with_prefix(&prefix, deadline)
                    .await?;
                let repo_ids = keys
                    .iter()
                    .filter_map(|key| key.strip_prefix(&prefix))
                    .filter_map(|key| key.split_once('/').map(|(repo_id, _)| repo_id.to_string()))
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>();
                Self::multi_repo_task(context, source, SqsRepoAction::Remove, &repo_ids, deadline)
                    .await
            }
            Self::MultiRepoAdd { source, repo_ids } => {
                Self::multi_repo_task(context, source, SqsRepoAction::Add, repo_ids, deadline).await
            }
            Self::MultiRepoRemove { source, repo_ids } => {
                Self::multi_repo_task(context, source, SqsRepoAction::Remove, repo_ids, deadline)
                    .await
            }
            Self::RepoAdd { source, repo_id } => {
                let git_remote = GitRemote::from_source(context, source)?;
                let default_branch = git_remote.repo_default_branch(repo_id, deadline).await?;
                Self::ref_commit(
                    context,
                    source,
                    repo_id,
                    &default_branch,
                    SqsRefType::Branch,
                    CommitInfo::default(),
                    deadline,
                )
                .await
            }
            Self::RepoRemove { source, repo_id } => {
                let git_remote = GitRemote::from_source(context, source)?;
                let repo_prefix = S3Keys::repo_prefix(&git_remote, repo_id)?;
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
                let git_remote = GitRemote::from_source(context, source)?;
                let ref_key = S3Keys::ref_key(&git_remote, repo_id, ref_name, *ref_type)?;
                context.aws.s3.delete(&ref_key, deadline).await
            }
        }
    }

    async fn multi_repo_task(
        context: &GwhContext,
        source: &GitSource,
        action: SqsRepoAction,
        repo_ids: &[String],
        deadline: SystemTime,
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
        Ok(context.aws.sqs.send_batch(&messages, deadline).await?)
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
        let git_remote = GitRemote::from_source(context, source)?;
        let ref_key = S3Keys::ref_key(&git_remote, repo_id, ref_name, ref_type)?;
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
