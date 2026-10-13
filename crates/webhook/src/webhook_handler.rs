use crate::{
    context::{Converters, RsWebhookContext},
    error::{RsWebhookError, RsWebhookResult},
    event_response::EventResponse,
    metrics::RequestMetrics,
    responses::Responses,
};
use http::Response;
use lambda_http::{Body, Request, RequestExt};
use ref_sync_shared::{request_context::RequestContext, timeouts::Deadline};

pub struct WebhookHandler;

enum Outcome {
    Respond(Response<Body>),
    Ignored,
    Unauthorized,
    Rejected,
}

impl Outcome {
    fn into_response(self) -> RsWebhookResult<Response<Body>> {
        match self {
            Self::Respond(response) => Ok(response),
            Self::Ignored => Responses::no_content(),
            Self::Unauthorized => Responses::unauthorized(),
            Self::Rejected => Responses::bad_request(),
        }
    }
}

impl WebhookHandler {
    pub async fn handle(
        &self,
        context: &RsWebhookContext,
        event: Request,
    ) -> RsWebhookResult<Response<Body>> {
        let metrics = RequestMetrics::start(event.uri().path());
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
        let outcome = self.convert(&request_context, &converters, &event).await?;
        if !matches!(outcome, Outcome::Unauthorized) {
            return outcome.into_response();
        }
        // The cached GITHUB_WEBHOOK_SECRET may be stale after a rotation.
        // Refresh once and retry before rejecting the delivery.
        tracing::warn!("webhook signature rejected; refreshing secrets and retrying");
        match request_context.context.refresh_converters().await {
            Ok(converters) => self
                .convert(&request_context, &converters, &event)
                .await?
                .into_response(),
            Err(error) => {
                tracing::warn!("failed to refresh secrets after signature rejection: {error}");
                Responses::unauthorized()
            }
        }
    }

    async fn convert(
        &self,
        request_context: &RequestContext<&RsWebhookContext>,
        converters: &Converters,
        event: &Request,
    ) -> RsWebhookResult<Outcome> {
        let mut ignored = false;
        let mut unauthorized = false;
        for converter in converters.iter() {
            match converter.event_response(event) {
                Ok(EventResponse::Accept(message)) => {
                    request_context
                        .context
                        .aws
                        .sqs
                        .send_batch(&[message], request_context.deadline)
                        .await?;
                    return Ok(Outcome::Respond(Responses::accepted()?));
                }
                Ok(EventResponse::Pong) => {
                    return Ok(Outcome::Respond(Responses::ok("pong")?));
                }
                Ok(EventResponse::Ignore) => ignored = true,
                Ok(EventResponse::Unauthorized) => unauthorized = true,
                Ok(EventResponse::WrongGitProvider) => continue,
                Err(RsWebhookError::Serde(error)) => {
                    tracing::warn!("rejecting malformed payload: {error}");
                    return Ok(Outcome::Respond(Responses::bad_request()?));
                }
                Err(RsWebhookError::InvalidHost(url)) => {
                    tracing::warn!("rejecting payload with unparseable host URL: {url:?}");
                    return Ok(Outcome::Respond(Responses::bad_request()?));
                }
                Err(error) => return Err(error),
            }
        }
        Ok(if unauthorized {
            Outcome::Unauthorized
        } else if ignored {
            Outcome::Ignored
        } else {
            Outcome::Rejected
        })
    }
}
