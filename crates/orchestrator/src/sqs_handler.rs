use crate::{
    context::RsOrchestratorContext, errors::error::RsOrchestratorResult, tasks::task::Task,
};
use aws_lambda_events::event::sqs::{SqsBatchResponse, SqsEvent, SqsMessage as SqsRecord};
use futures::{StreamExt, stream};
use lambda_runtime::LambdaEvent;
use ref_sync_shared::{
    deadline::Deadline,
    errors::{
        error::RsSharedError, invalid_message::RsInvalidMessageError, retryability::Retryability,
    },
    metrics::route::RouteMetrics,
    request_context::RequestContext,
    sqs_message::SqsMessage,
};

pub struct SqsHandler;

impl SqsHandler {
    pub async fn handle(
        &self,
        context: &RsOrchestratorContext,
        event: LambdaEvent<SqsEvent>,
    ) -> RsOrchestratorResult<SqsBatchResponse> {
        let request_context = RequestContext {
            context,
            deadline: Deadline::new(event.context.deadline()),
        };
        let process_concurrency = context.sqs_process_concurrency;
        let results = stream::iter(event.payload.records)
            .map(|record| {
                let request_context = request_context.clone();
                async move {
                    let result = self.handle_sqs_record(request_context, &record).await;
                    (record, result)
                }
            })
            .buffer_unordered(process_concurrency)
            .collect::<Vec<_>>()
            .await;
        let mut response = SqsBatchResponse::default();
        for (record, result) in results {
            if let Err(error) = result {
                if error.retryability() == Retryability::Permanent {
                    tracing::error!(
                        message_id = record.message_id.as_deref().unwrap_or("unknown"),
                        %error,
                        "Discarding SQS message with a permanent error",
                    );
                    continue;
                }
                let message_id = record.message_id.ok_or_else(|| {
                    RsSharedError::InvalidMessage(RsInvalidMessageError::MissingMessageId)
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
        request_context: RequestContext<&RsOrchestratorContext>,
        record: &SqsRecord,
    ) -> RsOrchestratorResult<()> {
        let body = record
            .body
            .as_deref()
            .ok_or_else(|| RsSharedError::InvalidMessage(RsInvalidMessageError::MissingBody))?;
        let message: SqsMessage = serde_json::from_str(body)?;
        let task = Task::from(message);
        let metrics = RouteMetrics::start(task.route());
        let result = task.handle(request_context).await;
        match &result {
            Ok(()) => metrics.record_success(),
            Err(error) => metrics.record_error(error.error_type()),
        }
        result
    }
}
