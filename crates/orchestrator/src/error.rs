use aws_credential_types::provider::error::CredentialsError;
use aws_sdk_dynamodb::operation::{
    delete_item::DeleteItemError, get_item::GetItemError, put_item::PutItemError,
    update_item::UpdateItemError,
};
use aws_sdk_ecs::operation::run_task::RunTaskError;
use aws_sdk_s3::operation::{
    delete_object::DeleteObjectError, delete_objects::DeleteObjectsError,
    list_objects_v2::ListObjectsV2Error,
};
use aws_sdk_sqs::operation::{
    send_message::SendMessageError, send_message_batch::SendMessageBatchError,
};
use aws_sigv4::{http_request::SigningError, sign::v4::signing_params::BuildError};
use http::header::{InvalidHeaderName, InvalidHeaderValue};
use lambda_http::lambda_runtime::Diagnostic;
use ref_sync_shared::error::RefSyncSharedError;

pub type RsOrchestratorResult<T> = Result<T, RsOrchestratorError>;

#[derive(Debug, thiserror::Error)]
pub enum RsOrchestratorError {
    #[error("Build: {0}")]
    Build(aws_smithy_types::error::operation::BuildError),
    #[error("Config: {0}")]
    Config(String),
    #[error("DynamoDbDeleteItem: {0}")]
    DynamoDbDeleteItem(Box<DeleteItemError>),
    #[error("DynamoDbGetItem: {0}")]
    DynamoDbGetItem(Box<GetItemError>),
    #[error("DynamoDbPutItem: {0}")]
    DynamoDbPutItem(Box<PutItemError>),
    #[error("DynamoDbUpdateItem: {0}")]
    DynamoDbUpdateItem(Box<UpdateItemError>),
    #[error("EnvVar: {0}")]
    EnvVar(Box<std::env::VarError>),
    #[error("GitHub: {0}")]
    GitHub(String),
    #[error("Http error: {0}")]
    Http(#[from] http::Error),
    #[error("Idempotency claim in progress: {0}")]
    IdempotencyInProgress(String),
    #[error("InternalServer: {0}")]
    InternalServer(String),
    #[error("Invalid credentials: {0}")]
    InvalidCredentials(#[from] CredentialsError),
    #[error("Invalid header name: {0}")]
    InvalidHeaderName(#[from] InvalidHeaderName),
    #[error("Invalid header value: {0}")]
    InvalidHeaderValue(#[from] InvalidHeaderValue),
    #[error("InvalidMessage: {0}")]
    InvalidMessage(String),
    #[error("Missing credentials")]
    MissingCredentials,
    #[error("Reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),
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
    #[error("S3DeleteObject: {0}")]
    S3DeleteObject(Box<DeleteObjectError>),
    #[error("S3DeleteObjects: {0}")]
    S3DeleteObjects(Box<DeleteObjectsError>),
    #[error("S3DeleteObjectsPartial: {0}")]
    S3DeleteObjectsPartial(String),
    #[error("S3ListObjects: {0}")]
    S3ListObjects(Box<ListObjectsV2Error>),
    #[error("S3Presign: {0}")]
    S3Presign(String),
    #[error("Serde error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("SQS error: {0}")]
    SendSqsMessage(#[from] Box<SendMessageError>),
    #[error("SQS batch error: {0}")]
    SendSqsMessageBatch(Box<SendMessageBatchError>),
    #[error("SQS batch partial failure: {0}")]
    SendSqsMessageBatchPartial(String),
    #[error("Signing params error: {0}")]
    SigningParams(#[from] BuildError),
    #[error("Signing error: {0}")]
    Signing(#[from] SigningError),
    #[error("Failed to set tracing subscriber: {0}")]
    SetTracingSubscriber(#[from] tracing::subscriber::SetGlobalDefaultError),
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
