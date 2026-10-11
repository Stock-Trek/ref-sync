use crate::error::{RsOrchestratorError, RsOrchestratorResult};
use http::{Response, StatusCode};
use lambda_http::Body;
use serde::Serialize;

pub struct Responses;

impl Responses {
    pub fn ok(body: &str) -> RsOrchestratorResult<Response<Body>> {
        Response::builder()
            .status(StatusCode::OK)
            .body(Body::Text(body.into()))
            .map_err(RsOrchestratorError::Http)
    }

    pub fn ok_json<T>(jsonable: T) -> RsOrchestratorResult<Response<Body>>
    where
        T: Serialize,
    {
        let json = serde_json::to_string(&jsonable)?;
        Response::builder()
            .status(StatusCode::OK)
            .header(http::header::CONTENT_TYPE, "application/json")
            .body(Body::Text(json))
            .map_err(RsOrchestratorError::Http)
    }

    pub fn accepted() -> RsOrchestratorResult<Response<Body>> {
        Response::builder()
            .status(StatusCode::ACCEPTED)
            .body(Body::Empty)
            .map_err(RsOrchestratorError::Http)
    }

    pub fn no_content() -> RsOrchestratorResult<Response<Body>> {
        Response::builder()
            .status(StatusCode::NO_CONTENT)
            .body(Body::Empty)
            .map_err(RsOrchestratorError::Http)
    }

    pub fn bad_request() -> RsOrchestratorResult<Response<Body>> {
        Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::Empty)
            .map_err(RsOrchestratorError::Http)
    }

    pub fn unauthorized() -> RsOrchestratorResult<Response<Body>> {
        Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .body(Body::Empty)
            .map_err(RsOrchestratorError::Http)
    }
}
