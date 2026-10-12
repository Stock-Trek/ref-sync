use crate::errors::retryability::Retryability;

#[derive(Debug, thiserror::Error)]
pub enum RsInternalServerError {
    #[error("lock poisoned: {resource}")]
    LockPoisoned { resource: &'static str },
    #[error("DynamoDB update_item failed: {0}")]
    DynamoDbUpdateItem(#[source] Box<dyn std::error::Error + Send + Sync + 'static>),
}

impl RsInternalServerError {
    pub fn retryability(&self) -> Retryability {
        match self {
            Self::LockPoisoned { .. } => Retryability::Permanent,
            Self::DynamoDbUpdateItem(_) => Retryability::Retryable,
        }
    }
}
