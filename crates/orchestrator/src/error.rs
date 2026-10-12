use aws_sdk_ecs::{error::BuildError, operation::run_task::RunTaskError};
use lambda_runtime::Diagnostic;
use ref_sync_shared::error::{ConfigError, RefSyncSharedError, Retryability, TimeoutError};

pub type RsOrchestratorResult<T> = Result<T, RsOrchestratorError>;

#[derive(Debug, thiserror::Error)]
pub enum RsOrchestratorError {
    #[error("Config: {0}")]
    Config(#[from] ConfigError),
    #[error("InvalidMessage: {0}")]
    InvalidMessage(#[from] InvalidMessageError),
    #[error("Invalid build agent network configuration: {0}")]
    NetworkConfiguration(#[from] BuildError),
    #[error("RefSyncShared error: {0}")]
    RefSyncShared(#[from] RefSyncSharedError),
    #[error("Run task error: {0}")]
    RunTask(#[from] Box<RunTaskError>),
    #[error("Run task failure: {0}")]
    RunTaskFailure(#[from] RunTaskFailure),
    #[error("Run error: {0}")]
    Run(#[from] Box<dyn std::error::Error + Send + Sync>),
    #[error("S3Presign: {0}")]
    S3Presign(#[from] S3PresignError),
    #[error("Serde error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("Timeout: {0}")]
    Timeout(#[from] TimeoutError),
}

impl RsOrchestratorError {
    pub fn retryability(&self) -> Retryability {
        match self {
            Self::Config(_)
            | Self::InvalidMessage(_)
            | Self::NetworkConfiguration(_)
            | Self::Serde(_) => Retryability::Permanent,
            Self::RefSyncShared(error) => error.retryability(),
            Self::RunTask(_)
            | Self::RunTaskFailure(_)
            | Self::Run(_)
            | Self::S3Presign(_)
            | Self::Timeout(_) => Retryability::Retryable,
        }
    }
}

/// Errors caused by a malformed inbound message.
#[derive(Debug, thiserror::Error)]
pub enum InvalidMessageError {
    #[error("invalid path: {value:?}")]
    InvalidPath { value: String },
    #[error("SQS record is missing a message id; cannot report a batch item failure")]
    MissingMessageId,
    #[error("SQS record is missing a body")]
    MissingBody,
}

/// Errors returned when ECS could not start the requested build task.
#[derive(Debug, thiserror::Error)]
pub enum RunTaskFailure {
    #[error("RunTask returned no tasks")]
    NoTasks,
    #[error("RunTask reported {} failure(s): {}", .0.len(), format_failures(.0))]
    Failures(Vec<TaskFailure>),
}

#[derive(Debug)]
pub struct TaskFailure {
    pub arn: Option<String>,
    pub reason: Option<String>,
    pub detail: Option<String>,
}

impl std::fmt::Display for TaskFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{}: {} ({})",
            self.arn.as_deref().unwrap_or("unknown task"),
            self.reason.as_deref().unwrap_or("unknown reason"),
            self.detail.as_deref().unwrap_or("no detail"),
        )
    }
}

fn format_failures(failures: &[TaskFailure]) -> String {
    failures
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("; ")
}

/// Errors raised while creating an S3 presigned URL.
#[derive(Debug, thiserror::Error)]
pub enum S3PresignError {
    #[error("invalid presigning config: {0}")]
    Config(#[from] aws_sdk_s3::presigning::PresigningConfigError),
    #[error("failed to presign request: {0}")]
    Presign(Box<aws_sdk_s3::error::SdkError<aws_sdk_s3::operation::put_object::PutObjectError>>),
}

impl From<RsOrchestratorError> for Diagnostic {
    fn from(value: RsOrchestratorError) -> Self {
        Diagnostic {
            error_type: "RsOrchestratorError".into(),
            error_message: value.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_errors_are_permanent() {
        let error =
            RsOrchestratorError::Serde(serde_json::from_str::<serde_json::Value>("{").unwrap_err());
        assert_eq!(error.retryability(), Retryability::Permanent);
    }

    #[test]
    fn invalid_message_errors_are_permanent() {
        let error = RsOrchestratorError::InvalidMessage(InvalidMessageError::MissingBody);
        assert_eq!(error.retryability(), Retryability::Permanent);
    }

    #[test]
    fn timeout_errors_are_retryable() {
        let error = RsOrchestratorError::Timeout(TimeoutError::DeadlineExceeded);
        assert_eq!(error.retryability(), Retryability::Retryable);
    }

    #[test]
    fn shared_config_errors_stay_permanent() {
        let error = RsOrchestratorError::RefSyncShared(RefSyncSharedError::Config(
            ConfigError::missing("EXAMPLE"),
        ));
        assert_eq!(error.retryability(), Retryability::Permanent);
    }
}
