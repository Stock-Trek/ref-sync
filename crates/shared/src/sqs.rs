use crate::{
    env_vars::EnvVars,
    error::{RefSyncSharedError, SharedResult},
    sqs_message::SqsMessage,
    timeouts::Timeouts,
};
use aws_config::SdkConfig;
use aws_sdk_sqs::{Client as SqsClient, types::SendMessageBatchRequestEntry};
use std::time::SystemTime;

const SQS_QUEUE_URL: &str = "SQS_QUEUE_URL";
const SQS_MAX_BATCH_SIZE: usize = 10;

#[derive(Clone)]
pub struct SqsConfig {
    pub queue_url: String,
}

impl SqsConfig {
    pub fn from_env() -> SharedResult<Self> {
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
    pub fn new(sdk_config: &SdkConfig, timeouts: Timeouts) -> SharedResult<Self> {
        Ok(Self {
            client: SqsClient::new(sdk_config),
            config: SqsConfig::from_env()?,
            timeouts,
        })
    }

    pub async fn send_batch(
        &self,
        messages: &[SqsMessage],
        deadline: SystemTime,
    ) -> SharedResult<()> {
        for batch in messages.chunks(SQS_MAX_BATCH_SIZE) {
            self.send_batch_chunk(batch, deadline).await?;
        }
        Ok(())
    }

    async fn send_batch_chunk(
        &self,
        messages: &[SqsMessage],
        deadline: SystemTime,
    ) -> SharedResult<()> {
        let entries = messages
            .iter()
            .enumerate()
            .map(|(index, message)| {
                SendMessageBatchRequestEntry::builder()
                    .id(index.to_string())
                    .message_body(serde_json::to_string(message)?)
                    .build()
                    .map_err(|error| RefSyncSharedError::Build(error.to_string()))
            })
            .collect::<SharedResult<Vec<_>>>()?;
        let timeout = self.timeouts.operation_for(deadline)?;
        let output = tokio::time::timeout(
            timeout,
            self.client
                .send_message_batch()
                .queue_url(&self.config.queue_url)
                .set_entries(Some(entries))
                .send(),
        )
        .await
        .map_err(|_| RefSyncSharedError::Timeout("SQS send_message_batch timed out".into()))?
        .map_err(|error| {
            RefSyncSharedError::SqsSendMessageBatch(Box::new(error.into_service_error()))
        })?;
        let failures = output.failed();
        if !failures.is_empty() {
            let failures = failures
                .iter()
                .map(|failure| {
                    format!(
                        "id={:?} code={:?} message={:?}",
                        failure.id(),
                        failure.code(),
                        failure.message()
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            return Err(RefSyncSharedError::SqsSendMessageBatchPartial(format!(
                "Failed to send {} message(s): {failures}",
                output.failed().len()
            )));
        }
        Ok(())
    }
}
