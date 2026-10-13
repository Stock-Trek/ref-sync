use crate::{
    aws_ops::build_request::BuildRequest, context::RsOrchestratorContext,
    errors::error::RsOrchestratorResult, tasks::s3_keys::S3Keys,
};
use ref_sync_shared::{
    git_ops::git_remote::GitRemote,
    request_context::RequestContext,
    sqs_message::{GitSource, SqsDetail, SqsMessage, SqsRefType},
};

#[derive(Default)]
struct CommitInfo {
    hash: Option<String>,
    forced: bool,
}

pub enum Task {
    AllReposAdd {
        source: GitSource,
    },
    MultiRepoAdd {
        source: GitSource,
        repo_ids: Vec<String>,
    },
    RepoAdd {
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
}

impl Task {
    pub fn route(&self) -> &'static str {
        match self {
            Self::AllReposAdd { .. } => "all_repos_add",
            Self::MultiRepoAdd { .. } => "multi_repo_add",
            Self::RepoAdd { .. } => "repo_add",
            Self::RepoTransfer { .. } => "repo_transfer",
            Self::RefCommit { .. } => "ref_commit",
        }
    }

    pub async fn handle(
        &self,
        request_context: RequestContext<&RsOrchestratorContext>,
    ) -> RsOrchestratorResult<()> {
        match self {
            Self::AllReposAdd { source } => {
                let git_remote =
                    GitRemote::from_source(&request_context.context.github_auth, source)?;
                let repo_ids = git_remote.repo_ids(request_context.deadline).await?;
                Self::multi_repo_task(request_context, source, &repo_ids).await
            }
            Self::MultiRepoAdd { source, repo_ids } => {
                Self::multi_repo_task(request_context, source, repo_ids).await
            }
            Self::RepoAdd { source, repo_id } => {
                Self::repo_add(request_context, source, repo_id).await
            }
            Self::RepoTransfer { source, repo_id } => {
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
        }
    }

    async fn multi_repo_task(
        request_context: RequestContext<&RsOrchestratorContext>,
        source: &GitSource,
        repo_ids: &[String],
    ) -> RsOrchestratorResult<()> {
        let messages = repo_ids
            .iter()
            .map(|repo_id| SqsMessage {
                source: source.clone(),
                detail: SqsDetail::RepoAdd {
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
        let git_remote = GitRemote::from_source(&request_context.context.github_auth, source)?;
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
        let git_remote = GitRemote::from_source(&request_context.context.github_auth, source)?;
        let ref_key = S3Keys::ref_key(&git_remote, repo_id, ref_name, ref_type)?;
        let signed_post = request_context
            .context
            .aws
            .s3
            .signed_post(&ref_key, request_context.deadline)
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
                    signed_post,
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
            SqsDetail::AllReposAdd => Self::AllReposAdd { source },
            SqsDetail::MultiRepoAdd { repo_ids } => Self::MultiRepoAdd { source, repo_ids },
            SqsDetail::RepoAdd { repo_id } => Self::RepoAdd { source, repo_id },
            SqsDetail::RepoTransfer { repo_id } => Self::RepoTransfer { source, repo_id },
            SqsDetail::RefCreate {
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
            SqsDetail::Commit {
                repo_id,
                ref_name,
                ref_type,
                commit_hash,
                forced,
            } => Self::RefCommit {
                source,
                repo_id,
                ref_name,
                ref_type,
                commit_hash: Some(commit_hash),
                forced,
            },
        }
    }
}
