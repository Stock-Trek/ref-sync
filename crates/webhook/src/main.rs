use crate::{
    context::RsWebhookContext,
    error::{RsWebhookError, RsWebhookResult},
    webhook_handler::WebhookHandler,
};
use lambda_http::{Request, run, service_fn};
use ref_sync_shared::tracing_setup::TracingSetup;
use std::sync::Arc;

mod aws;
mod context;
mod dto;
mod error;
mod event_response;
mod infisical;
mod provider;
mod responses;
mod webhook_handler;

#[tokio::main]
async fn main() -> RsWebhookResult<()> {
    TracingSetup::setup()?;
    let context = Arc::new(RsWebhookContext::new().await?);
    let handler = {
        move |event: Request| {
            let context = context.clone();
            async move { WebhookHandler.handle(&context, event).await }
        }
    };
    run(service_fn(handler)).await.map_err(RsWebhookError::Run)
}
