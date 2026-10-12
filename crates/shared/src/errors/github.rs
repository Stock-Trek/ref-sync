use crate::errors::retryability::Retryability;

#[derive(Debug, thiserror::Error)]
pub enum RsGitHubError {
    #[error("failed to build GitHub client: {0}")]
    BuildClient(#[source] octocrab::Error),
    #[error("failed to create client for installation {installation_id}: {source}")]
    Installation {
        installation_id: u64,
        #[source]
        source: octocrab::Error,
    },
    #[error("failed to list installation repositories: {0}")]
    ListRepositories(#[source] octocrab::Error),
    #[error("failed to get repository {repo_id}: {source}")]
    GetRepository {
        repo_id: u64,
        #[source]
        source: octocrab::Error,
    },
    #[error("repository {repo_id} was not found: {source}")]
    RepositoryNotFound {
        repo_id: u64,
        #[source]
        source: octocrab::Error,
    },
    #[error("host {host:?} is not in the allowed GitHub hosts list")]
    HostNotAllowed { host: String },
}

impl RsGitHubError {
    pub fn retryability(&self) -> Retryability {
        match self {
            Self::BuildClient(_)
            | Self::RepositoryNotFound { .. }
            | Self::HostNotAllowed { .. } => Retryability::Permanent,
            Self::Installation { source, .. }
            | Self::ListRepositories(source)
            | Self::GetRepository { source, .. } => Self::octocrab_retryability(source),
        }
    }
}

impl RsGitHubError {
    fn octocrab_retryability(error: &octocrab::Error) -> Retryability {
        match error {
            octocrab::Error::GitHub { source, .. } => {
                let status = source.status_code.as_u16();
                if status == 403 || status == 408 || status == 429 || status >= 500 {
                    Retryability::Retryable
                } else if (400..500).contains(&status) {
                    Retryability::Permanent
                } else {
                    Retryability::Retryable
                }
            }
            octocrab::Error::UriParse { .. }
            | octocrab::Error::Uri { .. }
            | octocrab::Error::Installation { .. }
            | octocrab::Error::InstallationTokenInvalidAuth { .. }
            | octocrab::Error::InvalidHeaderValue { .. }
            | octocrab::Error::InvalidUtf8 { .. }
            | octocrab::Error::Serde { .. }
            | octocrab::Error::Json { .. }
            | octocrab::Error::SerdeUrlEncoded { .. }
            | octocrab::Error::JWT { .. } => Retryability::Permanent,
            _ => Retryability::Retryable,
        }
    }
}
