use crate::{
    context::RsWebhookContext,
    error::{RsWebhookError, RsWebhookResult},
    webhook_handler::WebhookHandler,
};
use lambda_http::{Request, service_fn};
use metrics_cloudwatch_embedded::lambda::service::run_http;
use ref_sync_shared::{env_vars::EnvVars, metrics::Metrics, tracing_setup::TracingSetup};
use std::sync::Arc;

mod aws;
mod context;
mod dto;
mod error;
mod event_response;
mod infisical;
mod metrics;
mod responses;
mod webhook_handler;

#[tokio::main]
async fn main() -> RsWebhookResult<()> {
    TracingSetup::setup()?;
    let recorder = metrics_cloudwatch_embedded::Builder::new()
        .cloudwatch_namespace("RefSync/Webhook")
        .with_dimension("function", EnvVars::function_name("ref-sync-webhook"))
        .lambda_cold_start_metric(ref_sync_shared::metrics::COLD_STARTS_TOTAL)
        .init()?;
    Metrics::describe();
    let context = Arc::new(RsWebhookContext::new().await?);
    let handler = {
        move |event: Request| {
            let context = context.clone();
            async move { WebhookHandler.handle(&context, event).await }
        }
    };
    run_http(recorder, service_fn(handler))
        .await
        .map_err(RsWebhookError::Run)
}
