use crate::error::{RsWebhookError, RsWebhookResult};
use http::{Response, StatusCode};
use lambda_http::Body;

pub struct Responses;

impl Responses {
    pub fn ok(body: &str) -> RsWebhookResult<Response<Body>> {
        Response::builder()
            .status(StatusCode::OK)
            .body(Body::Text(body.into()))
            .map_err(RsWebhookError::Http)
    }

    pub fn accepted() -> RsWebhookResult<Response<Body>> {
        Response::builder()
            .status(StatusCode::ACCEPTED)
            .body(Body::Empty)
            .map_err(RsWebhookError::Http)
    }

    pub fn no_content() -> RsWebhookResult<Response<Body>> {
        Response::builder()
            .status(StatusCode::NO_CONTENT)
            .body(Body::Empty)
            .map_err(RsWebhookError::Http)
    }

    pub fn bad_request() -> RsWebhookResult<Response<Body>> {
        Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::Empty)
            .map_err(RsWebhookError::Http)
    }

    pub fn unauthorized() -> RsWebhookResult<Response<Body>> {
        Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .body(Body::Empty)
            .map_err(RsWebhookError::Http)
    }
}
