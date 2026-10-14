use crate::{
    context::{Converters, RsWebhookContext},
    error::{RsWebhookError, RsWebhookResult},
    event_response::EventResponse,
    responses::Responses,
};
use http::{Response, StatusCode};
use lambda_http::{Body, Request, RequestExt};
use ref_sync_shared::{
    deadline::Deadline,
    errors::{error::RsSharedError, retryability::Retryability},
    metrics::event::EventMetrics,
    metrics::route::RouteMetrics,
    request_context::RequestContext,
};

const WEBHOOK_ROUTE: &str = "webhook";
const ERROR_TYPE_HTTP: &str = "http";

pub struct WebhookHandler;

impl WebhookHandler {
    pub async fn handle(
        &self,
        context: &RsWebhookContext,
        event: Request,
    ) -> RsWebhookResult<Response<Body>> {
        let metrics = RouteMetrics::start(WEBHOOK_ROUTE);
        let result = self.handle_request(context, event).await;
        match result {
            Ok(response) => {
                if response.status().is_success() {
                    metrics.record_success_status(response.status().as_u16());
                } else {
                    metrics.record_error_status(ERROR_TYPE_HTTP, response.status().as_u16());
                }
                Ok(response)
            }
            Err(RsWebhookError::SecretsUnavailable { retry_after }) => {
                let status = StatusCode::SERVICE_UNAVAILABLE;
                metrics.record_error_status("secrets_unavailable", status.as_u16());
                Responses::service_unavailable(retry_after)
            }
            Err(error) if error.retryability() == Retryability::Permanent => {
                tracing::error!("rejecting permanently failing webhook request: {error}");
                let status = StatusCode::BAD_REQUEST;
                metrics.record_error_status(error.error_type(), status.as_u16());
                Responses::bad_request()
            }
            Err(error) => {
                metrics.record_error_status(
                    error.error_type(),
                    StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                );
                Err(error)
            }
        }
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
        let converters = request_context
            .context
            .converters(request_context.deadline)
            .await?;
        let response = self.convert(&request_context, &converters, &event).await?;
        if response != EventResponse::Unauthorized {
            return Self::respond(response);
        }
        tracing::warn!("webhook signature rejected; attempting secret refresh");
        match request_context
            .context
            .refresh_converters(request_context.deadline)
            .await
        {
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
                Ok(EventResponse::Accept(messages)) => {
                    let delivery_id = messages
                        .first()
                        .and_then(|message| message.source.delivery_id());
                    if let Some(delivery_id) = delivery_id
                        && request_context.context.dedup().is_duplicate(delivery_id)
                    {
                        tracing::info!(delivery_id, "ignoring replayed webhook delivery");
                        EventMetrics::record_duplicate_event();
                        return Ok(EventResponse::Duplicate);
                    }
                    let send_result = request_context
                        .context
                        .aws
                        .sqs
                        .send_batch(&messages, request_context.deadline)
                        .await;
                    match send_result {
                        Ok(()) => return Ok(EventResponse::Accept(messages)),
                        Err(RsSharedError::Sqs(error)) if error.any_sent(messages.len()) => {
                            tracing::warn!(
                                "SQS queued part of the delivery; accepting it and reporting the remainder: {error}"
                            );
                            return Ok(EventResponse::Accept(messages));
                        }
                        Err(error) if error.retryability() == Retryability::Permanent => {
                            if let Some(delivery_id) = delivery_id {
                                request_context.context.dedup().forget(delivery_id);
                            }
                            tracing::warn!("rejecting message that cannot be sent: {error}");
                            return Ok(EventResponse::Rejected);
                        }
                        Err(error) => {
                            if let Some(delivery_id) = delivery_id {
                                request_context.context.dedup().forget(delivery_id);
                            }
                            return Err(RsWebhookError::Shared(error));
                        }
                    }
                }
                Ok(EventResponse::Pong) => return Ok(EventResponse::Pong),
                Ok(EventResponse::Duplicate) => return Ok(EventResponse::Duplicate),
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
                Err(RsWebhookError::HostNotAllowed(url)) => {
                    tracing::warn!("rejecting payload from disallowed host: {url:?}");
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
            EventResponse::Duplicate => Responses::no_content(),
            EventResponse::Ignore => Responses::no_content(),
            EventResponse::Unauthorized => Responses::unauthorized(),
            EventResponse::WrongGitProvider | EventResponse::Rejected => Responses::bad_request(),
        }
    }
}
