use crate::{
    errors::{
        error::{RsSharedError, RsSharedResult},
        github::RsGitHubError,
        invalid_message::RsInvalidMessageError,
        timeout::RsTimeoutError,
    },
    git_ops::git_host::GitHost,
    timeouts::{Deadline, Timeouts},
};
use async_trait::async_trait;
use octocrab::{Octocrab, models::InstallationId};
use serde::{Deserialize, Serialize};

const PROVIDER: &str = "github";
const REPOS_PER_PAGE: u32 = 100;

#[derive(Debug, Serialize)]
struct RepositoriesParams {
    per_page: u32,
    page: u32,
}

#[derive(Debug, Deserialize)]
struct InstallationRepositoriesResponse {
    repositories: Vec<GitHubRepo>,
}

#[derive(Debug, Clone, Deserialize)]
struct GitHubRepo {
    id: u64,
    default_branch: String,
}

#[derive(Clone)]
pub struct GitHubInstallation {
    client: Octocrab,
    host: String,
    installation_id: u64,
    timeouts: Timeouts,
}

#[async_trait]
impl GitHost for GitHubInstallation {
    fn provider(&self) -> &'static str {
        PROVIDER
    }

    fn host(&self) -> &str {
        &self.host
    }

    fn installation_id(&self) -> String {
        self.installation_id.to_string()
    }

    async fn repo_ids(&self, deadline: Deadline) -> RsSharedResult<Vec<String>> {
        let timeout = deadline.operation_timeout(&self.timeouts)?;
        let repos = tokio::time::timeout(timeout, self.repos())
            .await
            .map_err(|_| {
                RsSharedError::Timeout(RsTimeoutError::Operation {
                    operation: "GitHub list repositories",
                })
            })??;
        Ok(repos.into_iter().map(|repo| repo.id.to_string()).collect())
    }

    async fn repo_default_branch(
        &self,
        repo_id: &str,
        deadline: Deadline,
    ) -> RsSharedResult<String> {
        let repo_id: u64 = repo_id.parse().map_err(|error| {
            RsSharedError::InvalidMessage(RsInvalidMessageError::GitHubRepositoryId {
                repo_id: repo_id.to_owned(),
                source: error,
            })
        })?;
        let timeout = deadline.operation_timeout(&self.timeouts)?;
        let repo = tokio::time::timeout(timeout, self.repo(repo_id))
            .await
            .map_err(|_| {
                RsSharedError::Timeout(RsTimeoutError::Operation {
                    operation: "GitHub get repository",
                })
            })?;
        repo.map(|r| r.default_branch)
    }
}

impl GitHubInstallation {
    pub fn new(
        app: &Octocrab,
        host: &str,
        installation_id: u64,
        timeouts: Timeouts,
    ) -> RsSharedResult<Self> {
        let client = app
            .installation(InstallationId::from(installation_id))
            .map_err(|error| {
                RsSharedError::GitHub(RsGitHubError::Installation {
                    installation_id,
                    source: error,
                })
            })?;
        Ok(Self {
            client,
            host: host.to_owned(),
            installation_id,
            timeouts,
        })
    }
}

// helpers
impl GitHubInstallation {
    async fn repos(&self) -> RsSharedResult<Vec<GitHubRepo>> {
        let mut repos = Vec::new();
        let mut page = 1;
        loop {
            let params = RepositoriesParams {
                per_page: REPOS_PER_PAGE,
                page,
            };
            let response = self
                .client
                .get::<InstallationRepositoriesResponse, _, _>(
                    "/installation/repositories",
                    Some(&params),
                )
                .await
                .map_err(|error| RsSharedError::GitHub(RsGitHubError::ListRepositories(error)))?;
            let received = response.repositories.len();
            repos.extend(response.repositories);
            if received < REPOS_PER_PAGE as usize {
                break;
            }
            page += 1;
        }
        Ok(repos)
    }

    async fn repo(&self, repo_id: u64) -> RsSharedResult<GitHubRepo> {
        self.client
            .get::<GitHubRepo, _, _>(format!("/repositories/{repo_id}"), None::<&()>)
            .await
            .map_err(|error| {
                if Self::is_not_found(&error) {
                    RsSharedError::GitHub(RsGitHubError::RepositoryNotFound {
                        repo_id,
                        source: error,
                    })
                } else {
                    RsSharedError::GitHub(RsGitHubError::GetRepository {
                        repo_id,
                        source: error,
                    })
                }
            })
    }

    fn is_not_found(error: &octocrab::Error) -> bool {
        matches!(
            error,
            octocrab::Error::GitHub { source, .. } if source.status_code.as_u16() == 404
        )
    }
}
