use aws_sdk_sqs::operation::send_message_batch::SendMessageBatchError;

#[derive(Debug, thiserror::Error)]
pub enum RsSqsError {
    #[error("SQS Batch error: {0}")]
    Batch(Box<SendMessageBatchError>),
    #[error("SQS PartialBatch failed to send {} message(s): {}", .failures.len(), Self::format_batch_failures(.failures))]
    PartialBatch { failures: Vec<SqsBatchFailure> },
}

impl RsSqsError {
    fn format_batch_failures(failures: &[SqsBatchFailure]) -> String {
        failures
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

#[derive(Debug)]
pub struct SqsBatchFailure {
    pub id: String,
    pub code: String,
    pub message: Option<String>,
}

impl std::fmt::Display for SqsBatchFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "id={:?} code={:?} message={:?}",
            self.id, self.code, self.message
        )
    }
}
