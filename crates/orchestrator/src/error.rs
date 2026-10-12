use aws_sdk_ecs::operation::run_task::RunTaskError;
use lambda_runtime::Diagnostic;
use ref_sync_shared::error::RefSyncSharedError;

pub type RsOrchestratorResult<T> = Result<T, RsOrchestratorError>;

#[derive(Debug, thiserror::Error)]
pub enum RsOrchestratorError {
    #[error("Config: {0}")]
    Config(String),
    #[error("InvalidMessage: {0}")]
    InvalidMessage(String),
    #[error("Invalid build agent network configuration: {0}")]
    NetworkConfiguration(String),
    #[error("RefSyncShared error: {0}")]
    RefSyncShared(#[from] RefSyncSharedError),
    #[error("Run task error: {0}")]
    RunTask(#[from] Box<RunTaskError>),
    #[error("Run task failure: {0}")]
    RunTaskFailure(String),
    #[error("Run error: {0}")]
    Run(#[from] Box<dyn std::error::Error + Send + Sync>),
    #[error("S3Presign: {0}")]
    S3Presign(String),
    #[error("Serde error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("Timeout: {0}")]
    Timeout(String),
}

impl From<RsOrchestratorError> for Diagnostic {
    fn from(value: RsOrchestratorError) -> Self {
        Diagnostic {
            error_type: "RsOrchestratorError".into(),
            error_message: value.to_string(),
        }
    }
}
