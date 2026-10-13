use crate::{
    context::{Converters, RsWebhookContext},
    error::{RsWebhookError, RsWebhookResult},
    event_response::EventResponse,
    metrics::{RequestMetrics, WEBHOOK_ROUTE},
    responses::Responses,
};
use http::Response;
use lambda_http::{Body, Request, RequestExt};
use ref_sync_shared::{
    deadline::Deadline, errors::retryability::Retryability, request_context::RequestContext,
};

pub struct WebhookHandler;

impl WebhookHandler {
    pub async fn handle(
        &self,
        context: &RsWebhookContext,
        event: Request,
    ) -> RsWebhookResult<Response<Body>> {
        let metrics = RequestMetrics::start(WEBHOOK_ROUTE);
        let result = self.handle_request(context, event).await;
        match &result {
            Ok(response) => metrics.record_success(response.status()),
            Err(error) => metrics.record_error(error.error_type()),
        }
        result
    }

    async fn handle_request(
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
        let converters = request_context.context.converters().await?;
        let response = self.convert(&request_context, &converters, &event).await?;
        if response != EventResponse::Unauthorized {
            return Self::respond(response);
        }
        tracing::warn!("webhook signature rejected; attempting secret refresh");
        match request_context.context.refresh_converters().await {
            Ok(Some(converters)) => {
                let response = self.convert(&request_context, &converters, &event).await?;
                Self::respond(response)
            }
            Ok(None) => {
                tracing::debug!("secret refresh throttled; rejecting webhook without retry");
                Responses::unauthorized()
            }
            Err(error) => {
                tracing::warn!("failed to refresh secrets after signature rejection: {error}");
                Responses::unauthorized()
            }
        }
    }

    /// Reduces the response of every converter into a single outcome, sending
    /// accepted messages to SQS as they are produced.
    async fn convert(
        &self,
        request_context: &RequestContext<&RsWebhookContext>,
        converters: &Converters,
        event: &Request,
    ) -> RsWebhookResult<EventResponse> {
        let mut ignored = false;
        let mut unauthorized = false;
        for converter in converters.iter() {
            match converter.event_response(event) {
                Ok(EventResponse::Accept(message)) => {
                    let send_result = request_context
                        .context
                        .aws
                        .sqs
                        .send_batch(std::slice::from_ref(&message), request_context.deadline)
                        .await;
                    match send_result {
                        Ok(()) => return Ok(EventResponse::Accept(message)),
                        Err(error) if error.retryability() == Retryability::Permanent => {
                            tracing::warn!("rejecting message that cannot be sent: {error}");
                            return Ok(EventResponse::Rejected);
                        }
                        Err(error) => return Err(RsWebhookError::Shared(error)),
                    }
                }
                Ok(EventResponse::Pong) => return Ok(EventResponse::Pong),
                Ok(EventResponse::Ignore) => ignored = true,
                Ok(EventResponse::Unauthorized) => unauthorized = true,
                Ok(EventResponse::WrongGitProvider) => continue,
                Ok(EventResponse::Rejected) => return Ok(EventResponse::Rejected),
                Err(RsWebhookError::Serde(error)) => {
                    tracing::warn!("rejecting malformed payload: {error}");
                    return Ok(EventResponse::Rejected);
                }
                Err(RsWebhookError::InvalidHost(url)) => {
                    tracing::warn!("rejecting payload with unparseable host URL: {url:?}");
                    return Ok(EventResponse::Rejected);
                }
                Err(error) => return Err(error),
            }
        }
        Ok(if unauthorized {
            EventResponse::Unauthorized
        } else if ignored {
            EventResponse::Ignore
        } else {
            EventResponse::Rejected
        })
    }

    fn respond(response: EventResponse) -> RsWebhookResult<Response<Body>> {
        match response {
            EventResponse::Accept(_) => Responses::accepted(),
            EventResponse::Pong => Responses::ok("pong"),
            EventResponse::Ignore => Responses::no_content(),
            EventResponse::Unauthorized => Responses::unauthorized(),
            EventResponse::WrongGitProvider | EventResponse::Rejected => Responses::bad_request(),
        }
    }
}
