use crate::errors::{
    archive::RsArchiveError, run_task::RsRunTaskError, s3_presign::RsS3PresignError,
};
use aws_sdk_ecs::operation::run_task::RunTaskError;
use lambda_runtime::Diagnostic;
use ref_sync_shared::errors::{error::RsSharedError, retryability::Retryability};

pub type RsOrchestratorResult<T> = Result<T, RsOrchestratorError>;

#[derive(Debug, thiserror::Error)]
pub enum RsOrchestratorError {
    #[error("Shared error: {0}")]
    Shared(#[from] RsSharedError),
    #[error("Run task error: {0}")]
    RunTask(#[from] Box<RunTaskError>),
    #[error("Run task failure: {0}")]
    RunTaskFailure(#[from] RsRunTaskError),
    #[error("Run error: {0}")]
    Run(#[from] Box<dyn std::error::Error + Send + Sync>),
    #[error("S3Presign: {0}")]
    S3Presign(#[from] RsS3PresignError),
    #[error("Archive: {0}")]
    Archive(#[from] RsArchiveError),
    #[error("Serde error: {0}")]
    Serde(#[from] serde_json::Error),
}

impl RsOrchestratorError {
    pub fn error_type(&self) -> &'static str {
        match self {
            Self::Shared(_) => "shared",
            Self::RunTask(_) => "run_task",
            Self::RunTaskFailure(_) => "run_task_failure",
            Self::Run(_) => "run",
            Self::S3Presign(_) => "s3_presign",
            Self::Archive(_) => "archive",
            Self::Serde(_) => "serde",
        }
    }

    pub fn retryability(&self) -> Retryability {
        match self {
            Self::Serde(_) | Self::Archive(_) => Retryability::Permanent,
            Self::Shared(error) => error.retryability(),
            Self::RunTask(error) => Self::run_task_retryability(error),
            Self::RunTaskFailure(error) => error.retryability(),
            Self::S3Presign(error) => error.retryability(),
            Self::Run(_) => Retryability::Retryable,
        }
    }

    fn run_task_retryability(error: &RunTaskError) -> Retryability {
        match error {
            RunTaskError::AccessDeniedException(_)
            | RunTaskError::BlockedException(_)
            | RunTaskError::ClientException(_)
            | RunTaskError::ClusterNotFoundException(_)
            | RunTaskError::ConflictException(_)
            | RunTaskError::InvalidParameterException(_)
            | RunTaskError::PlatformTaskDefinitionIncompatibilityException(_)
            | RunTaskError::PlatformUnknownException(_)
            | RunTaskError::UnsupportedFeatureException(_) => Retryability::Permanent,
            _ => Retryability::Retryable,
        }
    }
}

impl From<RsOrchestratorError> for Diagnostic {
    fn from(value: RsOrchestratorError) -> Self {
        Diagnostic {
            error_type: value.error_type().into(),
            error_message: value.to_string(),
        }
    }
}
