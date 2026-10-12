use crate::{errors::retryability::Retryability, sqs_message::SqsMessage};

#[derive(Debug, thiserror::Error)]
pub enum RsSqsError {
    #[error("SQS PartialBatch failed to send {} message(s): {}", .failures.len(), Self::format_batch_failures(.failures))]
    PartialBatch { failures: Vec<SqsBatchFailure> },
    #[error(
        "SQS message body is {size} bytes, which exceeds the maximum batch payload of {limit} bytes"
    )]
    MessageTooLarge { size: usize, limit: usize },
}

impl RsSqsError {
    fn format_batch_failures(failures: &[SqsBatchFailure]) -> String {
        failures
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub fn retryability(&self) -> Retryability {
        match self {
            Self::PartialBatch { .. } => Retryability::Retryable,
            Self::MessageTooLarge { .. } => Retryability::Permanent,
        }
    }
}

/// A message that could not be sent, together with the reason.
#[derive(Debug)]
pub struct SqsBatchFailure {
    /// The source message, matched back from the entry id we sent. `None` when
    /// the service reported an id we did not issue.
    pub source: Option<SqsMessage>,
    /// The error code reported by SQS.
    pub code: String,
    /// The human-readable error message reported by SQS.
    pub detail: Option<String>,
}

impl std::fmt::Display for SqsBatchFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "source={:?} code={:?} detail={:?}",
            self.source, self.code, self.detail
        )
    }
}
