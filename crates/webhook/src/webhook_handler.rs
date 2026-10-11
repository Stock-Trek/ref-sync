use crate::{
    context::GwhContext,
    dto::event_converter::EventConverter,
    error::{RsWebhookError, RsWebhookResult},
    event_response::EventResponse,
    provider::GitProvider,
    responses::Responses,
};
use http::Response;
use lambda_http::{Body, Request, RequestExt};
use strum::IntoEnumIterator;

pub struct WebhookHandler;

impl WebhookHandler {
    pub async fn handle(
        &self,
        context: &GwhContext,
        event: Request,
    ) -> RsWebhookResult<Response<Body>> {
        let deadline = event.lambda_context().deadline();
        let mut ignored = false;
        let mut unauthorized = false;
        for provider in GitProvider::iter() {
            match provider.event_response(&context.webhook, &event) {
                Ok(response) => match response {
                    EventResponse::Accept(message) => {
                        context.aws.sqs.send_batch(&[message], deadline).await?;
                        return Responses::accepted();
                    }
                    EventResponse::Pong => return Responses::ok("pong"),
                    EventResponse::Ignore => ignored = true,
                    EventResponse::Unauthorized => unauthorized = true,
                    EventResponse::WrongGitProvider => continue,
                },
                Err(RsWebhookError::Serde(error)) => {
                    tracing::error!("Failed to deserialize signed event: {error}");
                    ignored = true;
                }
                Err(error) => return Err(error),
            }
        }
        if unauthorized {
            Responses::unauthorized()
        } else if ignored {
            Responses::no_content()
        } else {
            Responses::bad_request()
        }
    }
}
