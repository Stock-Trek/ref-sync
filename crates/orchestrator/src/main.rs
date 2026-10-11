use crate::{
    context::GwhContext,
    error::{RsOrchestratorError, RsOrchestratorResult},
    sqs_handler::SqsHandler,
};
use aws_lambda_events::event::sqs::SqsEvent;
use lambda_runtime::{LambdaEvent, run, service_fn};
use ref_sync_shared::tracing_setup::TracingSetup;
use std::sync::Arc;

mod aws_ops;
mod constants;
mod context;
mod error;
mod sqs_handler;
mod tasks;

#[tokio::main]
async fn main() -> RsOrchestratorResult<()> {
    TracingSetup::setup()?;
    let context = Arc::new(GwhContext::new().await?);
    let handler = {
        move |event: LambdaEvent<SqsEvent>| {
            let context = context.clone();
            async move { SqsHandler.handle(&context, event).await }
        }
    };
    run(service_fn(handler))
        .await
        .map_err(RsOrchestratorError::Run)
}
