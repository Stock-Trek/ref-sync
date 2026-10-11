use crate::{
    context::GwhContext,
    error::{RsWebhookResult, RsWebhookError},
    webhook_handler::WebhookHandler,
};
use lambda_http::{Request, run, service_fn};
use ref_sync_shared::tracing_setup::TracingSetup;
use std::sync::Arc;

mod aws_ops;
mod constants;
mod context;
mod dto;
mod error;
mod event_response;
mod git_ops;
mod infisical;
mod provider;
mod responses;
mod sqs_event;
mod tasks;
mod webhook_handler;

#[tokio::main]
async fn main() -> RsWebhookResult<()> {
    TracingSetup::setup()?;
    let context = Arc::new(GwhContext::new().await?);
    let handler = {
        move |event: Request| {
            let context = context.clone();
            async move { WebhookHandler.handle(&context, event).await }
        }
    };
    run(service_fn(handler)).await.map_err(RsWebhookError::Run)
}
