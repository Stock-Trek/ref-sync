use aws_sdk_dynamodb::{error::ProvideErrorMetadata, operation::update_item::UpdateItemError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Retryability {
    Retryable,
    Permanent,
}

impl Retryability {
    pub fn dynamodb_update_item_retryability(error: &UpdateItemError) -> Retryability {
        match error {
            UpdateItemError::ConditionalCheckFailedException(_)
            | UpdateItemError::InvalidEndpointException(_)
            | UpdateItemError::ItemCollectionSizeLimitExceededException(_)
            | UpdateItemError::ResourceNotFoundException(_) => Retryability::Permanent,
            error if Self::is_permanent_dynamodb_error_code(error.code()) => {
                Retryability::Permanent
            }
            _ => Retryability::Retryable,
        }
    }
}

impl Retryability {
    fn is_permanent_dynamodb_error_code(code: Option<&str>) -> bool {
        matches!(
            code,
            Some(
                "AccessDeniedException"
                    | "ConditionalCheckFailedException"
                    | "IdempotentParameterMismatchException"
                    | "InvalidEndpointException"
                    | "ItemCollectionSizeLimitExceededException"
                    | "ResourceInUseException"
                    | "ResourceNotFoundException"
                    | "ValidationException"
            )
        )
    }

    pub fn sqs_batch_failure_retryability(code: &str) -> Retryability {
        if Self::is_permanent_sqs_error_code(code) {
            Retryability::Permanent
        } else {
            Retryability::Retryable
        }
    }

    fn is_permanent_sqs_error_code(code: &str) -> bool {
        matches!(
            code,
            "AccessDenied"
                | "BatchEntryIdsNotDistinct"
                | "BatchRequestTooLong"
                | "EmptyBatchRequest"
                | "InvalidAddress"
                | "InvalidBatchEntryId"
                | "InvalidMessageContents"
                | "InvalidSecurity"
                | "KmsAccessDenied"
                | "KmsDisabled"
                | "KmsInvalidKeyUsage"
                | "KmsInvalidState"
                | "KmsNotFound"
                | "KmsOptInRequired"
                | "QueueDoesNotExist"
                | "TooManyEntriesInBatchRequest"
                | "UnsupportedOperation"
        )
    }
}
