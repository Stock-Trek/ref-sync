use crate::error::{RsWebhookError, RsWebhookResult};
use http::{Response, StatusCode};
use lambda_http::Body;
use serde::Serialize;

pub struct Responses;

impl Responses {
    pub fn ok(body: &str) -> RsWebhookResult<Response<Body>> {
        Response::builder()
            .status(StatusCode::OK)
            .body(Body::Text(body.into()))
            .map_err(RsWebhookError::Http)
    }

    pub fn ok_json<T>(jsonable: T) -> RsWebhookResult<Response<Body>>
    where
        T: Serialize,
    {
        let json = serde_json::to_string(&jsonable)?;
        Response::builder()
            .status(StatusCode::OK)
            .header(http::header::CONTENT_TYPE, "application/json")
            .body(Body::Text(json))
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
