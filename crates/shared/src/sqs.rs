use crate::{
    env_vars::EnvVars,
    error::{
        RsSharedError, RsSharedResult, SqsBatchFailure, SqsSendMessageBatchPartialError,
        TimeoutError,
    },
    sqs_message::SqsMessage,
    timeouts::{Deadline, Timeouts},
};
use aws_config::SdkConfig;
use aws_sdk_sqs::{Client as SqsClient, types::SendMessageBatchRequestEntry};

const SQS_QUEUE_URL: &str = "SQS_QUEUE_URL";
const SQS_MAX_BATCH_SIZE: usize = 10;

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

    pub async fn send_batch(
        &self,
        messages: &[SqsMessage],
        deadline: Deadline,
    ) -> RsSharedResult<()> {
        for batch in messages.chunks(SQS_MAX_BATCH_SIZE) {
            self.send_batch_chunk(batch, deadline).await?;
        }
        Ok(())
    }

    async fn send_batch_chunk(
        &self,
        messages: &[SqsMessage],
        deadline: Deadline,
    ) -> RsSharedResult<()> {
        let entries = messages
            .iter()
            .enumerate()
            .map(|(index, message)| {
                SendMessageBatchRequestEntry::builder()
                    .id(index.to_string())
                    .message_body(serde_json::to_string(message)?)
                    .build()
                    .map_err(RsSharedError::Build)
            })
            .collect::<RsSharedResult<Vec<_>>>()?;
        let timeout = deadline.operation_timeout(&self.timeouts)?;
        let output = tokio::time::timeout(
            timeout,
            self.client
                .send_message_batch()
                .queue_url(&self.config.queue_url)
                .set_entries(Some(entries))
                .send(),
        )
        .await
        .map_err(|_| {
            RsSharedError::Timeout(TimeoutError::Operation {
                operation: "SQS send_message_batch",
            })
        })?
        .map_err(|error| {
            RsSharedError::SqsSendMessageBatch(Box::new(error.into_service_error()))
        })?;
        let failed = output.failed();
        if !failed.is_empty() {
            let failures = failed
                .iter()
                .map(|failure| SqsBatchFailure {
                    id: failure.id().to_owned(),
                    code: failure.code().to_owned(),
                    message: failure.message().map(str::to_owned),
                })
                .collect();
            return Err(RsSharedError::SqsSendMessageBatchPartial(
                SqsSendMessageBatchPartialError { failures },
            ));
        }
        Ok(())
    }
}
