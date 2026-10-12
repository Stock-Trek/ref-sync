use crate::{
    constants::SQS_PROCESS_CONCURRENCY,
    context::GwhContext,
    error::{RsOrchestratorError, RsOrchestratorResult},
    tasks::task::Task,
};
use aws_lambda_events::event::sqs::{SqsBatchResponse, SqsEvent, SqsMessage as SqsRecord};
use futures::{StreamExt, stream};
use lambda_runtime::LambdaEvent;
use ref_sync_shared::sqs_message::SqsMessage;
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
                let message_id = record.message_id.ok_or_else(|| {
                    RsOrchestratorError::InvalidMessage(
                        "SQS record is missing a message id; cannot report a batch item failure"
                            .into(),
                    )
                })?;
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
        Task::from(message).handle(context, deadline).await
    }
}
