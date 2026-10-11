use crate::{
    constants::SQS_PROCESS_CONCURRENCY,
    context::GwhContext,
    error::{RsOrchestratorError, RsOrchestratorResult},
    tasks::task::Task,
};
use aws_lambda_events::event::sqs::{SqsBatchResponse, SqsEvent, SqsMessage as SqsRecord};
use futures::{StreamExt, stream};
use lambda_runtime::LambdaEvent;
use ref_sync_shared::{
    dynamodb::{DynamoDb, IdempotencyClaim},
    sqs_message::SqsMessage,
};
use std::time::SystemTime;

pub struct SqsHandler;

impl SqsHandler {
    pub async fn handle(
        &self,
        context: &GwhContext,
        event: LambdaEvent<SqsEvent>,
    ) -> RsOrchestratorResult<SqsBatchResponse> {
        let deadline = event.context.deadline();
        let results = stream::iter(event.payload.records)
            .map(|record| async move {
                let result = self.handle_sqs_record(context, deadline, &record).await;
                (record, result)
            })
            .buffer_unordered(SQS_PROCESS_CONCURRENCY)
            .collect::<Vec<_>>()
            .await;
        let mut response = SqsBatchResponse::default();
        for (record, result) in results {
            if let Err(error) = result {
                let message_id = record.message_id.unwrap_or_default();
                tracing::error!(
                    message_id = %message_id,
                    %error,
                    "Failed to process SQS message, reporting batch item failure",
                );
                response.add_failure(message_id);
            }
        }
        Ok(response)
    }
}

impl SqsHandler {
    async fn handle_sqs_record(
        &self,
        context: &GwhContext,
        deadline: SystemTime,
        record: &SqsRecord,
    ) -> RsOrchestratorResult<()> {
        let body = record.body.as_deref().ok_or_else(|| {
            RsOrchestratorError::InvalidMessage("SQS record is missing a body".into())
        })?;
        let message: SqsMessage = serde_json::from_str(body)?;
        let message_id = record.message_id.clone().unwrap_or_default();
        let Some(idempotency_key) = message.idempotency_key() else {
            return Task::from(message).handle(context, deadline).await;
        };
        let lease_expires_at = match context
            .aws
            .dynamodb
            .claim_idempotency(&idempotency_key, deadline)
            .await?
        {
            IdempotencyClaim::Acquired { lease_expires_at } => lease_expires_at,
            IdempotencyClaim::AlreadyDone => {
                tracing::info!(
                    message_id = %message_id,
                    delivery = %idempotency_key,
                    "Skipping duplicate delivery",
                );
                return Ok(());
            }
            IdempotencyClaim::InProgress => {
                tracing::info!(
                    message_id = %message_id,
                    delivery = %idempotency_key,
                    "Idempotency claim is in progress, retrying delivery",
                );
                return Err(RsOrchestratorError::IdempotencyInProgress(idempotency_key));
            }
        };
        let mut claim = IdempotencyGuard::new(
            context.aws.dynamodb.clone(),
            idempotency_key,
            lease_expires_at,
            deadline,
            message_id.clone(),
        );
        let result = Task::from(message).handle(context, deadline).await;
        match result {
            Ok(()) => {
                claim.complete().await?;
                Ok(())
            }
            Err(error) => {
                if let Err(release_error) = claim.release().await {
                    tracing::warn!(
                        message_id = %message_id,
                        error = %release_error,
                        "Failed to release idempotency claim after task failure",
                    );
                }
                Err(error)
            }
        }
    }
}

/// Releases an idempotency claim on drop unless it is disarmed.
///
/// Moving the release into `Drop` means a panic while handling the task still
/// releases the claim instead of leaving it held until the lease expires.
/// `Drop` cannot be async, so the release is spawned onto the current Tokio
/// runtime.
struct IdempotencyGuard {
    dynamodb: DynamoDb,
    key: String,
    lease_expires_at: i64,
    deadline: SystemTime,
    message_id: String,
    armed: bool,
}

impl IdempotencyGuard {
    fn new(
        dynamodb: DynamoDb,
        key: String,
        lease_expires_at: i64,
        deadline: SystemTime,
        message_id: String,
    ) -> Self {
        Self {
            dynamodb,
            key,
            lease_expires_at,
            deadline,
            message_id,
            armed: true,
        }
    }

    async fn complete(&mut self) -> RsOrchestratorResult<()> {
        self.dynamodb
            .complete_idempotency(&self.key, self.lease_expires_at, self.deadline)
            .await?;
        self.armed = false;
        Ok(())
    }

    async fn release(&mut self) -> RsOrchestratorResult<()> {
        self.dynamodb
            .release_idempotency(&self.key, self.lease_expires_at, self.deadline)
            .await?;
        self.armed = false;
        Ok(())
    }
}

impl Drop for IdempotencyGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let dynamodb = self.dynamodb.clone();
        let key = std::mem::take(&mut self.key);
        let message_id = std::mem::take(&mut self.message_id);
        let lease_expires_at = self.lease_expires_at;
        let deadline = self.deadline;
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            tracing::warn!(
                %message_id,
                %key,
                "Failed to release idempotency claim: no Tokio runtime available",
            );
            return;
        };
        handle.spawn(async move {
            if let Err(error) = dynamodb
                .release_idempotency(&key, lease_expires_at, deadline)
                .await
            {
                tracing::warn!(
                    %message_id,
                    %error,
                    "Failed to release idempotency claim",
                );
            }
        });
    }
}
