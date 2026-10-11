use crate::{
    context::GwhContext,
    error::{RsOrchestratorError, RsOrchestratorResult},
    sqs_handler::SqsHandler,
};
use lambda_http::{Request, run, service_fn};
use ref_sync_shared::tracing_setup::TracingSetup;
use std::sync::Arc;

mod aws_ops;
mod constants;
mod context;
mod error;
mod provider;
mod responses;
mod sqs_event;
mod sqs_handler;
mod tasks;

#[tokio::main]
async fn main() -> RsOrchestratorResult<()> {
    TracingSetup::setup()?;
    let context = Arc::new(GwhContext::new().await?);
    let handler = {
        move |event: Request| {
            let context = context.clone();
            async move { SqsHandler.handle(&context, event).await }
        }
    };
    run(service_fn(handler))
        .await
        .map_err(RsOrchestratorError::Run)
}
