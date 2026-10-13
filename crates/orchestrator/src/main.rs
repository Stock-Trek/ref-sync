use crate::{
    context::RsOrchestratorContext,
    errors::error::{RsOrchestratorError, RsOrchestratorResult},
    sqs_handler::SqsHandler,
};
use aws_lambda_events::event::sqs::SqsEvent;
use lambda_runtime::{LambdaEvent, service_fn};
use metrics_cloudwatch_embedded::lambda::service::run;
use ref_sync_shared::{env_vars::EnvVars, metrics::Metrics, tracing_setup::TracingSetup};
use std::sync::Arc;

mod aws_ops;
mod context;
mod errors;
mod metrics;
mod sqs_handler;
mod tasks;

#[tokio::main]
async fn main() -> RsOrchestratorResult<()> {
    TracingSetup::setup()?;
    let recorder = metrics_cloudwatch_embedded::Builder::new()
        .cloudwatch_namespace("RefSync/Orchestrator")
        .with_dimension("function", EnvVars::function_name("ref-sync-orchestrator"))
        .lambda_cold_start_metric(ref_sync_shared::metrics::COLD_STARTS_TOTAL)
        .init()?;
    Metrics::describe();
    let context = Arc::new(RsOrchestratorContext::new().await?);
    let handler = {
        move |event: LambdaEvent<SqsEvent>| {
            let context = context.clone();
            async move { SqsHandler.handle(&context, event).await }
        }
    };
    run(recorder, service_fn(handler))
        .await
        .map_err(RsOrchestratorError::Run)
}
