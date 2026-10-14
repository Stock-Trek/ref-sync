use aws_credential_types::provider::error::CredentialsError;
use aws_sigv4::{http_request::SigningError, sign::v4::signing_params::BuildError};
use http::header::{InvalidHeaderName, InvalidHeaderValue, ToStrError};
use lambda_http::lambda_runtime::Diagnostic;
use ref_sync_shared::errors::{error::RsSharedError, retryability::Retryability};

const HTTP_REQUEST_TIMEOUT: u16 = 408;
const HTTP_TOO_MANY_REQUESTS: u16 = 429;
const HTTP_INTERNAL_SERVER_ERROR: u16 = 500;

pub type RsWebhookResult<T> = Result<T, RsWebhookError>;

#[derive(Debug, thiserror::Error)]
pub enum RsWebhookError {
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
    #[error("Invalid host URL: {0:?}")]
    InvalidHost(String),
    #[error("Missing credentials")]
    MissingCredentials,
    #[error("Refresh throttled")]
    RefreshThrottled,
    #[error("Reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("Shared error: {0}")]
    Shared(#[from] RsSharedError),
    #[error("Run error: {0}")]
    Run(#[from] Box<dyn std::error::Error + Send + Sync>),
    #[error("Serde error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("Signing params error: {0}")]
    SigningParams(#[from] BuildError),
    #[error("Signing error: {0}")]
    Signing(#[from] SigningError),
}

impl RsWebhookError {
    pub fn retryability(&self) -> Retryability {
        match self {
            Self::Http(_)
            | Self::InvalidHeaderName(_)
            | Self::InvalidHeaderValue(_)
            | Self::InvalidHeaderValueStr(_)
            | Self::InvalidHost(_)
            | Self::MissingCredentials
            | Self::RefreshThrottled
            | Self::Serde(_)
            | Self::SigningParams(_)
            | Self::Signing(_) => Retryability::Permanent,
            Self::InvalidCredentials(error) => Self::credentials_retryability(error),
            Self::Reqwest(error) => Self::reqwest_retryability(error),
            Self::Shared(error) => error.retryability(),
            Self::Run(_) => Retryability::Retryable,
        }
    }

    fn credentials_retryability(error: &CredentialsError) -> Retryability {
        match error {
            CredentialsError::ProviderTimedOut(_) | CredentialsError::ProviderError(_) => {
                Retryability::Retryable
            }
            _ => Retryability::Permanent,
        }
    }

    fn reqwest_retryability(error: &reqwest::Error) -> Retryability {
        match error.status().map(|status| status.as_u16()) {
            Some(HTTP_REQUEST_TIMEOUT | HTTP_TOO_MANY_REQUESTS) => Retryability::Retryable,
            Some(status) if status >= HTTP_INTERNAL_SERVER_ERROR => Retryability::Retryable,
            Some(status) if (400..500).contains(&status) => Retryability::Permanent,
            _ => Retryability::Retryable,
        }
    }

    pub fn error_type(&self) -> &'static str {
        match self {
            Self::Http(_) => "http",
            Self::InvalidCredentials(_) => "invalid_credentials",
            Self::InvalidHeaderName(_) => "invalid_header_name",
            Self::InvalidHeaderValue(_) => "invalid_header_value",
            Self::InvalidHeaderValueStr(_) => "invalid_header_value_str",
            Self::InvalidHost(_) => "invalid_host",
            Self::MissingCredentials => "missing_credentials",
            Self::RefreshThrottled => "refresh_throttled",
            Self::Reqwest(_) => "reqwest",
            Self::Shared(_) => "shared",
            Self::Run(_) => "run",
            Self::Serde(_) => "serde",
            Self::SigningParams(_) => "signing_params",
            Self::Signing(_) => "signing",
        }
    }
}

impl From<RsWebhookError> for Diagnostic {
    fn from(value: RsWebhookError) -> Self {
        Diagnostic {
            error_type: value.error_type().into(),
            error_message: value.to_string(),
        }
    }
}
