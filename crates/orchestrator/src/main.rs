use crate::{
    context::RsOrchestratorContext,
    errors::error::{RsOrchestratorError, RsOrchestratorResult},
    sqs_handler::SqsHandler,
};
use aws_lambda_events::event::sqs::SqsEvent;
use lambda_runtime::{LambdaEvent, service_fn};
use metrics_cloudwatch_embedded::lambda::service::run;
use ref_sync_shared::{
    env_vars::EnvVarConfig, metrics::descriptions::DescriptionsMetrics, tracing_setup::TracingSetup,
};
use std::sync::Arc;

mod aws_ops;
mod context;
mod errors;
mod sqs_handler;
mod tasks;

const DEFAULT_FUNCTION_NAME: &str = "ref-sync-orchestrator";

#[tokio::main]
async fn main() -> RsOrchestratorResult<()> {
    let config = EnvVarConfig::from_env();
    TracingSetup::setup(&config)?;
    let recorder = metrics_cloudwatch_embedded::Builder::new()
        .cloudwatch_namespace("RefSync/Orchestrator")
        .with_dimension(
            "function",
            config.lambda_function_name(DEFAULT_FUNCTION_NAME)?,
        )
        .lambda_cold_start_metric(ref_sync_shared::metrics::descriptions::COLD_STARTS_TOTAL)
        .init()?;
    DescriptionsMetrics::describe();
    let context = Arc::new(RsOrchestratorContext::new(&config).await?);
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
