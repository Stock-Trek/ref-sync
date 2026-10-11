use crate::error::{GwhError, GwhResult};
use http::{Response, StatusCode};
use lambda_http::Body;
use serde::Serialize;

pub struct Responses;

impl Responses {
    pub fn ok(body: &str) -> GwhResult<Response<Body>> {
        Response::builder()
            .status(StatusCode::OK)
            .body(Body::Text(body.into()))
            .map_err(GwhError::Http)
    }

    pub fn ok_json<T>(jsonable: T) -> GwhResult<Response<Body>>
    where
        T: Serialize,
    {
        let json = serde_json::to_string(&jsonable)?;
        Response::builder()
            .status(StatusCode::OK)
            .header(http::header::CONTENT_TYPE, "application/json")
            .body(Body::Text(json))
            .map_err(GwhError::Http)
    }

    pub fn accepted() -> GwhResult<Response<Body>> {
        Response::builder()
            .status(StatusCode::ACCEPTED)
            .body(Body::Empty)
            .map_err(GwhError::Http)
    }

    pub fn no_content() -> GwhResult<Response<Body>> {
        Response::builder()
            .status(StatusCode::NO_CONTENT)
            .body(Body::Empty)
            .map_err(GwhError::Http)
    }

    pub fn bad_request() -> GwhResult<Response<Body>> {
        Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::Empty)
            .map_err(GwhError::Http)
    }

    pub fn unauthorized() -> GwhResult<Response<Body>> {
        Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .body(Body::Empty)
            .map_err(GwhError::Http)
    }
}
