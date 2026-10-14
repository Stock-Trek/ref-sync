use crate::{
    deadline::Deadline,
    errors::error::RsSharedResult,
    git_ops::github::{github_auth::GitHubAuth, installation::GitHubInstallation},
    sqs_message::GitSource,
};

pub enum GitRemote {
    GitHub(GitHubInstallation),
}

#[allow(async_fn_in_trait)]
pub trait GitHost<InstallationId, RepoId>: Send + Sync {
    fn provider(&self) -> &'static str;
    fn host(&self) -> &str;
    fn installation_id(&self) -> InstallationId;
    fn parse_repo_id(&self, repo_id: &str) -> RsSharedResult<RepoId>;
    async fn repo_ids(&self, deadline: Deadline) -> RsSharedResult<Vec<RepoId>>;
    async fn repo_default_branch(
        &self,
        repo_id: RepoId,
        deadline: Deadline,
    ) -> RsSharedResult<String>;
    async fn ref_commit(
        &self,
        repo_id: RepoId,
        ref_name: &str,
        deadline: Deadline,
    ) -> RsSharedResult<String>;
}

impl GitRemote {
    pub fn from_source(github_auth: &GitHubAuth, source: &GitSource) -> RsSharedResult<Self> {
        match source {
            GitSource::GitHub {
                host,
                installation_id,
                ..
            } => Ok(GitRemote::GitHub(
                github_auth.installation(host, *installation_id)?,
            )),
        }
    }
}

impl GitHost<u64, u64> for GitRemote {
    fn provider(&self) -> &'static str {
        match self {
            GitRemote::GitHub(installation) => installation.provider(),
        }
    }

    fn host(&self) -> &str {
        match self {
            GitRemote::GitHub(installation) => installation.host(),
        }
    }

    fn installation_id(&self) -> u64 {
        match self {
            GitRemote::GitHub(installation) => installation.installation_id(),
        }
    }

    fn parse_repo_id(&self, repo_id: &str) -> RsSharedResult<u64> {
        match self {
            GitRemote::GitHub(installation) => installation.parse_repo_id(repo_id),
        }
    }

    async fn repo_ids(&self, deadline: Deadline) -> RsSharedResult<Vec<u64>> {
        match self {
            GitRemote::GitHub(installation) => installation.repo_ids(deadline).await,
        }
    }

    async fn repo_default_branch(
        &self,
        repo_id: u64,
        deadline: Deadline,
    ) -> RsSharedResult<String> {
        match self {
            GitRemote::GitHub(installation) => {
                installation.repo_default_branch(repo_id, deadline).await
            }
        }
    }

    async fn ref_commit(
        &self,
        repo_id: u64,
        ref_name: &str,
        deadline: Deadline,
    ) -> RsSharedResult<String> {
        match self {
            GitRemote::GitHub(installation) => {
                installation.ref_commit(repo_id, ref_name, deadline).await
            }
        }
    }
}
