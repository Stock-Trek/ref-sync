#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Retryability {
    Retryable,
    Permanent,
}

impl Retryability {
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
                | "BuildError"
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
