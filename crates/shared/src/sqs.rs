use crate::{
    deadline::Deadline,
    env_vars::EnvVarConfig,
    errors::{
        error::{RsSharedError, RsSharedResult},
        sqs::{RsSqsError, SqsBatchFailure},
    },
    metrics::sqs::SqsMetrics,
    sqs_message::SqsMessage,
    timeouts::Timeouts,
};
use aws_config::SdkConfig;
use aws_sdk_sqs::{
    Client as SqsClient, error::ProvideErrorMetadata, types::SendMessageBatchRequestEntry,
};
use std::time::Duration;

/// The most entries SQS accepts in a single `SendMessageBatch` request.
const SQS_MAX_BATCH_ENTRIES: usize = 10;
/// The most payload bytes (the sum of the message bodies) SQS accepts in a
/// single `SendMessageBatch` request.
pub(crate) const SQS_MAX_BATCH_BYTES: usize = 256 * 1024;

#[derive(Clone)]
pub struct SqsConfig {
    pub queue_url: String,
}

impl SqsConfig {
    pub fn from_env(config: &EnvVarConfig) -> RsSharedResult<Self> {
        Ok(Self {
            queue_url: config.sqs_queue_url()?,
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
    pub fn new(client: SqsClient, timeouts: Timeouts, config: SqsConfig) -> Self {
        Self {
            client,
            config,
            timeouts,
        }
    }

    pub fn from_env(
        sdk_config: &SdkConfig,
        timeouts: Timeouts,
        config: &EnvVarConfig,
    ) -> RsSharedResult<Self> {
        Ok(Self::new(
            SqsClient::new(sdk_config),
            timeouts,
            SqsConfig::from_env(config)?,
        ))
    }

    /// Sends every message, splitting them into as many service batches as the
    /// entry and payload limits require.
    ///
    /// Every batch is attempted even when an earlier one fails, so a failure
    /// does not leave later messages unsent. Once the deadline has passed no
    /// further batches are attempted and the messages in them are reported as
    /// unsent. If any message could not be sent, the returned error names each
    /// message and the reason it failed.
    pub async fn send_batch(
        &self,
        messages: &[SqsMessage],
        deadline: Deadline,
    ) -> RsSharedResult<()> {
        let bodies = messages
            .iter()
            .map(serde_json::to_string)
            .collect::<Result<Vec<_>, _>>()
            .inspect_err(|_| SqsMetrics::record_messages_failed(messages.len()))?;
        let batches = Self::batch_indices(&bodies)
            .inspect_err(|_| SqsMetrics::record_messages_failed(messages.len()))?;
        let mut failures = Vec::new();
        for (position, batch) in batches.iter().enumerate() {
            let timeout = match self.timeouts.operation_timeout(deadline) {
                Ok(timeout) => timeout,
                Err(error) => {
                    for remaining in &batches[position..] {
                        failures.extend(Self::batch_failures(
                            messages,
                            remaining,
                            "DeadlineExceeded",
                            error.to_string(),
                        ));
                    }
                    break;
                }
            };
            failures.extend(
                self.send_batch_chunk(messages, &bodies, batch, timeout)
                    .await,
            );
        }
        if failures.is_empty() {
            SqsMetrics::record_messages_sent(messages.len());
            Ok(())
        } else {
            SqsMetrics::record_messages_sent(messages.len() - failures.len());
            SqsMetrics::record_messages_failed(failures.len());
            Err(RsSharedError::Sqs(RsSqsError::PartialBatch { failures }))
        }
    }

    async fn send_batch_chunk(
        &self,
        messages: &[SqsMessage],
        bodies: &[String],
        indices: &[usize],
        timeout: Duration,
    ) -> Vec<SqsBatchFailure> {
        let entries = match indices
            .iter()
            .map(|&index| {
                SendMessageBatchRequestEntry::builder()
                    .id(index.to_string())
                    .message_body(&bodies[index])
                    .build()
            })
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(entries) => entries,
            Err(error) => {
                return Self::batch_failures(messages, indices, "BuildError", error.to_string());
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
            Ok(Ok(output)) => output
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
                .collect(),
            Ok(Err(error)) => {
                let error = error.into_service_error();
                let code = error.code().unwrap_or("RequestFailure").to_owned();
                Self::batch_failures(messages, indices, &code, error.to_string())
            }
            Err(_) => Self::batch_failures(
                messages,
                indices,
                "RequestTimeout",
                "SQS send_message_batch timed out".to_owned(),
            ),
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
