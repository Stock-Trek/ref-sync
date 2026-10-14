use crate::errors::retryability::Retryability;

const HTTP_FORBIDDEN: u16 = 403;
const HTTP_REQUEST_TIMEOUT: u16 = 408;
const HTTP_TOO_MANY_REQUESTS: u16 = 429;
const HTTP_INTERNAL_SERVER_ERROR: u16 = 500;

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
    #[error("host {host:?} is not a valid hostname, IP address, or host:port")]
    InvalidHost { host: String },
}

impl RsGitHubError {
    pub fn retryability(&self) -> Retryability {
        match self {
            Self::BuildClient(_)
            | Self::RepositoryNotFound { .. }
            | Self::HostNotAllowed { .. }
            | Self::InvalidHost { .. } => Retryability::Permanent,
            Self::Installation { source, .. }
            | Self::ListRepositories(source)
            | Self::GetRepository { source, .. } => Self::octocrab_retryability(source),
        }
    }
}

impl RsGitHubError {
    fn octocrab_retryability(error: &octocrab::Error) -> Retryability {
        match error {
            octocrab::Error::GitHub { source, .. } => Self::github_retryability(
                source.status_code.as_u16(),
                &source.message,
                source.documentation_url.as_deref(),
            ),
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

    fn github_retryability(
        status: u16,
        message: &str,
        documentation_url: Option<&str>,
    ) -> Retryability {
        match status {
            HTTP_FORBIDDEN if Self::is_rate_limit_error(message, documentation_url) => {
                Retryability::Retryable
            }
            HTTP_REQUEST_TIMEOUT | HTTP_TOO_MANY_REQUESTS => Retryability::Retryable,
            status if status >= HTTP_INTERNAL_SERVER_ERROR => Retryability::Retryable,
            status if (400..500).contains(&status) => Retryability::Permanent,
            _ => Retryability::Retryable,
        }
    }

    fn is_rate_limit_error(message: &str, documentation_url: Option<&str>) -> bool {
        Self::mentions_rate_limit(message)
            || documentation_url.is_some_and(Self::mentions_rate_limit)
    }

    fn mentions_rate_limit(value: &str) -> bool {
        let value = value.to_ascii_lowercase();
        value.contains("rate limit")
            || value.contains("rate-limit")
            || value.contains("rate_limit")
            || value.contains("ratelimit")
    }
}
