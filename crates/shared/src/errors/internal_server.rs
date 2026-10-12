#[derive(Debug, thiserror::Error)]
pub enum RsInternalServerError {
    #[error("DynamoDB update_item failed: {0}")]
    DynamoDbUpdateItem(#[source] Box<dyn std::error::Error + Send + Sync + 'static>),
}
