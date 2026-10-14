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
            Self::PartialBatch { failures }
                if !failures.is_empty() && failures.iter().all(SqsBatchFailure::is_permanent) =>
            {
                Retryability::Permanent
            }
            Self::PartialBatch { .. } => Retryability::Retryable,
            Self::MessageTooLarge { .. } => Retryability::Permanent,
        }
    }

    pub fn any_sent(&self, total: usize) -> bool {
        matches!(self, Self::PartialBatch { failures } if failures.len() < total)
    }
}

#[derive(Debug)]
pub struct SqsBatchFailure {
    pub source: Option<SqsMessage>,
    pub code: String,
    pub detail: Option<String>,
}

impl SqsBatchFailure {
    pub fn is_permanent(&self) -> bool {
        Retryability::sqs_batch_failure_retryability(&self.code) == Retryability::Permanent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failure() -> SqsBatchFailure {
        SqsBatchFailure {
            source: None,
            code: "RequestTimeout".to_owned(),
            detail: None,
        }
    }

    #[test]
    fn any_sent_when_some_messages_queued() {
        let error = RsSqsError::PartialBatch {
            failures: vec![failure()],
        };
        assert!(error.any_sent(2));
    }

    #[test]
    fn none_sent_when_all_messages_failed() {
        let error = RsSqsError::PartialBatch {
            failures: vec![failure(), failure()],
        };
        assert!(!error.any_sent(2));
    }

    #[test]
    fn none_sent_when_message_too_large() {
        let error = RsSqsError::MessageTooLarge { size: 1, limit: 0 };
        assert!(!error.any_sent(1));
    }
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
