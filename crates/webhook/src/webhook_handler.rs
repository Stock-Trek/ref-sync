use crate::{
    context::RsWebhookContext,
    error::{RsWebhookError, RsWebhookResult},
    event_response::EventResponse,
    responses::Responses,
};
use http::Response;
use lambda_http::{Body, Request, RequestExt};
use ref_sync_shared::{request_context::RequestContext, timeouts::Deadline};

pub struct WebhookHandler;

impl WebhookHandler {
    pub async fn handle(
        &self,
        context: &RsWebhookContext,
        event: Request,
    ) -> RsWebhookResult<Response<Body>> {
        let request_context = RequestContext {
            context,
            deadline: Deadline::new(event.lambda_context().deadline()),
        };
        self.handle_event(request_context, event).await
    }

    async fn handle_event(
        &self,
        request_context: RequestContext<&RsWebhookContext>,
        event: Request,
    ) -> RsWebhookResult<Response<Body>> {
        let mut ignored = false;
        let mut unauthorized = false;
        for converter in &request_context.context.converters {
            match converter.event_response(&event) {
                Ok(response) => match response {
                    EventResponse::Accept(message) => {
                        request_context
                            .context
                            .aws
                            .sqs
                            .send_batch(&[message], request_context.deadline)
                            .await?;
                        return Responses::accepted();
                    }
                    EventResponse::Pong => return Responses::ok("pong"),
                    EventResponse::Ignore => ignored = true,
                    EventResponse::Unauthorized => unauthorized = true,
                    EventResponse::WrongGitProvider => continue,
                },
                Err(RsWebhookError::Serde(error)) => {
                    tracing::warn!("rejecting malformed payload: {error}");
                    return Responses::bad_request();
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
