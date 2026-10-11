use aws_sdk_dynamodb::operation::{
    delete_item::DeleteItemError, get_item::GetItemError, put_item::PutItemError,
    update_item::UpdateItemError,
};

pub type SharedResult<T> = Result<T, RefSyncSharedError>;

#[derive(Debug, thiserror::Error)]
pub enum RefSyncSharedError {
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
    #[error("Serde error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("Failed to set tracing subscriber: {0}")]
    SetTracingSubscriber(#[from] tracing::subscriber::SetGlobalDefaultError),
    #[error("Timeout: {0}")]
    Timeout(String),
}
