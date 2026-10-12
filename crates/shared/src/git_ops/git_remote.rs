use crate::{
    errors::error::RsSharedResult,
    git_ops::github::{github_auth::GitHubAuth, installation::GitHubInstallation},
    sqs_message::GitSource,
    timeouts::Deadline,
};

pub enum GitRemote {
    GitHub(GitHubInstallation),
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

    pub fn provider(&self) -> &'static str {
        match self {
            GitRemote::GitHub(installation) => installation.provider(),
        }
    }

    pub fn host(&self) -> &str {
        match self {
            GitRemote::GitHub(installation) => installation.host(),
        }
    }

    pub fn installation_id(&self) -> String {
        match self {
            GitRemote::GitHub(installation) => installation.installation_id(),
        }
    }

    pub async fn repo_ids(&self, deadline: Deadline) -> RsSharedResult<Vec<String>> {
        match self {
            GitRemote::GitHub(installation) => installation.repo_ids(deadline).await,
        }
    }

    pub async fn repo_default_branch(
        &self,
        repo_id: &str,
        deadline: Deadline,
    ) -> RsSharedResult<String> {
        match self {
            GitRemote::GitHub(installation) => {
                installation.repo_default_branch(repo_id, deadline).await
            }
        }
    }
}
