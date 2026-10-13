use crate::errors::{
    config::RsConfigError, github::RsGitHubError, invalid_message::RsInvalidMessageError,
    retryability::Retryability, sqs::RsSqsError, timeout::RsTimeoutError,
};
use aws_sdk_sqs::error::BuildError;

pub type RsSharedResult<T> = Result<T, RsSharedError>;

#[derive(Debug, thiserror::Error)]
pub enum RsSharedError {
    #[error("Build: {0}")]
    Build(#[from] BuildError),
    #[error("Config: {0}")]
    Config(#[from] RsConfigError),
    #[error("EnvVar: {0}")]
    EnvVar(Box<std::env::VarError>),
    #[error("GitHub: {0}")]
    GitHub(#[from] RsGitHubError),
    #[error("InvalidMessage: {0}")]
    InvalidMessage(#[from] RsInvalidMessageError),
    #[error("Serde error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("Sqs error: {0}")]
    Sqs(RsSqsError),
    #[error("Failed to set tracing subscriber: {0}")]
    SetTracingSubscriber(#[from] tracing::subscriber::SetGlobalDefaultError),
    #[error("Timeout: {0}")]
    Timeout(#[from] RsTimeoutError),
}

impl RsSharedError {
    pub fn retryability(&self) -> Retryability {
        match self {
            Self::Build(_)
            | Self::Config(_)
            | Self::EnvVar(_)
            | Self::InvalidMessage(_)
            | Self::Serde(_)
            | Self::SetTracingSubscriber(_) => Retryability::Permanent,
            Self::GitHub(error) => error.retryability(),
            Self::Sqs(error) => error.retryability(),
            Self::Timeout(_) => Retryability::Retryable,
        }
    }

    pub fn is_retryable(&self) -> bool {
        self.retryability() == Retryability::Retryable
    }
}
