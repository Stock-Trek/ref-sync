use aws_credential_types::provider::error::CredentialsError;
use aws_sigv4::{http_request::SigningError, sign::v4::signing_params::BuildError};
use http::header::{InvalidHeaderName, InvalidHeaderValue, ToStrError};
use lambda_http::lambda_runtime::Diagnostic;
use ref_sync_shared::error::{ConfigError, RefSyncSharedError};

pub type RsWebhookResult<T> = Result<T, RsWebhookError>;

#[derive(Debug, thiserror::Error)]
pub enum RsWebhookError {
    #[error("Config: {0}")]
    Config(#[from] ConfigError),
    #[error("Http error: {0}")]
    Http(#[from] http::Error),
    #[error("Invalid credentials: {0}")]
    InvalidCredentials(#[from] CredentialsError),
    #[error("Invalid header name: {0}")]
    InvalidHeaderName(#[from] InvalidHeaderName),
    #[error("Invalid header value: {0}")]
    InvalidHeaderValue(#[from] InvalidHeaderValue),
    #[error("Invalid header value string: {0}")]
    InvalidHeaderValueStr(#[from] ToStrError),
    #[error("Missing credentials")]
    MissingCredentials,
    #[error("Reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("RefSyncShared error: {0}")]
    RefSyncShared(#[from] RefSyncSharedError),
    #[error("Run error: {0}")]
    Run(#[from] Box<dyn std::error::Error + Send + Sync>),
    #[error("Serde error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("Signing params error: {0}")]
    SigningParams(#[from] BuildError),
    #[error("Signing error: {0}")]
    Signing(#[from] SigningError),
}

impl From<RsWebhookError> for Diagnostic {
    fn from(value: RsWebhookError) -> Self {
        Diagnostic {
            error_type: "GwhError".into(),
            error_message: value.to_string(),
        }
    }
}
