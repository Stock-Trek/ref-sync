use crate::error::{RsOrchestratorError, RsOrchestratorResult};
use http::{Response, StatusCode};
use lambda_http::Body;
use serde::Serialize;

pub struct Responses;

impl Responses {
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
}
