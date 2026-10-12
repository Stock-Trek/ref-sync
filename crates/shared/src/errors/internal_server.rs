#[derive(Debug, thiserror::Error)]
pub enum RsInternalServerError {
    #[error("lock poisoned: {resource}")]
    LockPoisoned { resource: &'static str },
    #[error("DynamoDB update_item failed: {0}")]
    DynamoDbUpdateItem(#[source] Box<dyn std::error::Error + Send + Sync + 'static>),
}
