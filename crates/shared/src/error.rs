use aws_sdk_dynamodb::operation::put_item::PutItemError;
use aws_sdk_sqs::operation::send_message_batch::SendMessageBatchError;

pub type SharedResult<T> = Result<T, RefSyncSharedError>;

#[derive(Debug, thiserror::Error)]
pub enum RefSyncSharedError {
    #[error("Build: {0}")]
    Build(String),
    #[error("Config: {0}")]
    Config(String),
    #[error("DynamoDbPutItem: {0}")]
    DynamoDbPutItem(Box<PutItemError>),
    #[error("EnvVar: {0}")]
    EnvVar(Box<std::env::VarError>),
    #[error("GitHub: {0}")]
    GitHub(String),
    #[error("InternalServer: {0}")]
    InternalServer(String),
    #[error("Serde error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("SqsSendMessageBatch: {0}")]
    SqsSendMessageBatch(Box<SendMessageBatchError>),
    #[error("SqsSendMessageBatchPartial: {0}")]
    SqsSendMessageBatchPartial(String),
    #[error("Failed to set tracing subscriber: {0}")]
    SetTracingSubscriber(#[from] tracing::subscriber::SetGlobalDefaultError),
    #[error("Timeout: {0}")]
    Timeout(String),
}
