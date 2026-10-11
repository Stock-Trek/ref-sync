use crate::{
    aws_ops::dynamodb::{DynamoDb, IdempotencyClaim},
    constants::SQS_PROCESS_CONCURRENCY,
    context::GwhContext,
    error::{RsOrchestratorError, RsOrchestratorResult},
    responses::Responses,
    sqs_event::{SqsEvent, SqsEventResponse, SqsRecord},
    sqs_message::SqsMessage,
    tasks::task::Task,
};
use futures::{StreamExt, stream};
use http::Response;
use lambda_http::{Body, Request, RequestExt};
use std::time::SystemTime;

pub struct SqsHandler;

impl SqsHandler {
    pub async fn handle(
        &self,
        context: &GwhContext,
        event: Request,
    ) -> RsOrchestratorResult<Response<Body>> {
        let deadline = event.lambda_context().deadline();
        let sqs_event: SqsEvent = serde_json::from_slice(event.body())?;
        let results = stream::iter(sqs_event.records)
            .map(|record| async move {
                let result = self.handle_sqs_record(context, deadline, &record).await;
                (record, result)
            })
            .buffer_unordered(SQS_PROCESS_CONCURRENCY)
            .collect::<Vec<_>>()
            .await;
        let mut response = SqsEventResponse::default();
        for (record, result) in results {
            if let Err(error) = result {
                tracing::error!(
                    message_id = %record.message_id,
                    %error,
                    "Failed to process SQS message, reporting batch item failure",
                );
                response.add_failure(record.message_id.clone());
            }
        }
        Responses::ok_json(&response)
    }
}

impl SqsHandler {
    async fn handle_sqs_record(
        &self,
        context: &GwhContext,
        deadline: SystemTime,
        record: &SqsRecord,
    ) -> RsOrchestratorResult<()> {
        let message: SqsMessage = serde_json::from_str(&record.body)?;
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
                    message_id = %record.message_id,
                    delivery = %idempotency_key,
                    "Skipping duplicate delivery",
                );
                return Ok(());
            }
            IdempotencyClaim::InProgress => {
                tracing::info!(
                    message_id = %record.message_id,
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
            record.message_id.clone(),
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
                        message_id = %record.message_id,
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
