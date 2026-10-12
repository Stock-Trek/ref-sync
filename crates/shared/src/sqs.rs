use crate::{
    env_vars::EnvVars,
    errors::{
        error::{RsSharedError, RsSharedResult},
        sqs::{RsSqsError, SqsBatchFailure},
    },
    sqs_message::SqsMessage,
    timeouts::{Deadline, Timeouts},
};
use aws_config::SdkConfig;
use aws_sdk_sqs::{
    Client as SqsClient, error::ProvideErrorMetadata, types::SendMessageBatchRequestEntry,
};

const SQS_QUEUE_URL: &str = "SQS_QUEUE_URL";
/// The most entries SQS accepts in a single `SendMessageBatch` request.
const SQS_MAX_BATCH_ENTRIES: usize = 10;
/// The most payload bytes (the sum of the message bodies) SQS accepts in a
/// single `SendMessageBatch` request.
const SQS_MAX_BATCH_BYTES: usize = 256 * 1024;

#[derive(Clone)]
pub struct SqsConfig {
    pub queue_url: String,
}

impl SqsConfig {
    pub fn from_env() -> RsSharedResult<Self> {
        Ok(Self {
            queue_url: EnvVars::env_var(SQS_QUEUE_URL)?,
        })
    }
}

#[derive(Clone)]
pub struct Sqs {
    client: SqsClient,
    config: SqsConfig,
    timeouts: Timeouts,
}

impl Sqs {
    pub fn new(sdk_config: &SdkConfig, timeouts: Timeouts) -> RsSharedResult<Self> {
        Ok(Self {
            client: SqsClient::new(sdk_config),
            config: SqsConfig::from_env()?,
            timeouts,
        })
    }

    /// Sends every message, splitting them into as many service batches as the
    /// entry and payload limits require.
    ///
    /// Every batch is attempted even when an earlier one fails, so a failure
    /// does not leave later messages unsent. If any message could not be sent,
    /// the returned error names each message and the reason it failed.
    pub async fn send_batch(
        &self,
        messages: &[SqsMessage],
        deadline: Deadline,
    ) -> RsSharedResult<()> {
        let bodies = messages
            .iter()
            .map(serde_json::to_string)
            .collect::<Result<Vec<_>, _>>()?;
        let mut failures = Vec::new();
        for batch in Self::batch_indices(&bodies)? {
            failures.extend(
                self.send_batch_chunk(messages, &bodies, &batch, deadline)
                    .await?,
            );
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(RsSharedError::Sqs(RsSqsError::PartialBatch { failures }))
        }
    }

    async fn send_batch_chunk(
        &self,
        messages: &[SqsMessage],
        bodies: &[String],
        indices: &[usize],
        deadline: Deadline,
    ) -> RsSharedResult<Vec<SqsBatchFailure>> {
        let entries = indices
            .iter()
            .map(|&index| {
                SendMessageBatchRequestEntry::builder()
                    .id(index.to_string())
                    .message_body(&bodies[index])
                    .build()
                    .map_err(RsSharedError::Build)
            })
            .collect::<RsSharedResult<Vec<_>>>()?;
        let timeout = match deadline.operation_timeout(&self.timeouts) {
            Ok(timeout) => timeout,
            Err(error) => {
                return Ok(Self::batch_failures(
                    messages,
                    indices,
                    "DeadlineExceeded",
                    error.to_string(),
                ));
            }
        };
        let result = tokio::time::timeout(
            timeout,
            self.client
                .send_message_batch()
                .queue_url(&self.config.queue_url)
                .set_entries(Some(entries))
                .send(),
        )
        .await;
        match result {
            Ok(Ok(output)) => Ok(output
                .failed()
                .iter()
                .map(|failure| {
                    let index = failure.id().parse::<usize>().ok();
                    SqsBatchFailure {
                        source: index.and_then(|index| messages.get(index).cloned()),
                        code: failure.code().to_owned(),
                        detail: failure.message().map(str::to_owned),
                    }
                })
                .collect()),
            Ok(Err(error)) => {
                let error = error.into_service_error();
                let code = error.code().unwrap_or("RequestFailure").to_owned();
                Ok(Self::batch_failures(
                    messages,
                    indices,
                    &code,
                    error.to_string(),
                ))
            }
            Err(_) => Ok(Self::batch_failures(
                messages,
                indices,
                "RequestTimeout",
                "SQS send_message_batch timed out".to_owned(),
            )),
        }
    }

    fn batch_failures(
        messages: &[SqsMessage],
        indices: &[usize],
        code: &str,
        detail: String,
    ) -> Vec<SqsBatchFailure> {
        indices
            .iter()
            .map(|&index| SqsBatchFailure {
                source: messages.get(index).cloned(),
                code: code.to_owned(),
                detail: Some(detail.clone()),
            })
            .collect()
    }

    /// Splits the serialized message bodies into batches that respect both the
    /// entry count and the total payload size limits.
    fn batch_indices(bodies: &[String]) -> RsSharedResult<Vec<Vec<usize>>> {
        let mut batches = Vec::new();
        let mut current = Vec::new();
        let mut current_bytes = 0;
        for (index, body) in bodies.iter().enumerate() {
            let size = body.len();
            if size > SQS_MAX_BATCH_BYTES {
                return Err(RsSharedError::Sqs(RsSqsError::MessageTooLarge {
                    size,
                    limit: SQS_MAX_BATCH_BYTES,
                }));
            }
            if !current.is_empty()
                && (current.len() == SQS_MAX_BATCH_ENTRIES
                    || current_bytes + size > SQS_MAX_BATCH_BYTES)
            {
                batches.push(std::mem::take(&mut current));
                current_bytes = 0;
            }
            current.push(index);
            current_bytes += size;
        }
        if !current.is_empty() {
            batches.push(current);
        }
        Ok(batches)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bodies(sizes: &[usize]) -> Vec<String> {
        sizes.iter().map(|size| "x".repeat(*size)).collect()
    }

    #[test]
    fn batches_respect_the_entry_limit() {
        let bodies = bodies(&[1; 25]);
        let batches = Sqs::batch_indices(&bodies).unwrap();
        assert_eq!(batches.len(), 3);
        assert_eq!(batches[0], (0..10).collect::<Vec<_>>());
        assert_eq!(batches[1], (10..20).collect::<Vec<_>>());
        assert_eq!(batches[2], (20..25).collect::<Vec<_>>());
    }

    #[test]
    fn batches_respect_the_payload_limit() {
        let half = SQS_MAX_BATCH_BYTES / 2;
        let bodies = bodies(&[half, half, half]);
        let batches = Sqs::batch_indices(&bodies).unwrap();
        assert_eq!(batches, vec![vec![0, 1], vec![2]]);
    }

    #[test]
    fn batches_empty_input() {
        let batches = Sqs::batch_indices(&[]).unwrap();
        assert!(batches.is_empty());
    }

    #[test]
    fn rejects_an_oversized_message() {
        let bodies = bodies(&[SQS_MAX_BATCH_BYTES + 1]);
        assert!(matches!(
            Sqs::batch_indices(&bodies),
            Err(RsSharedError::Sqs(RsSqsError::MessageTooLarge { .. }))
        ));
    }
}
