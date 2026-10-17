use aws_credential_types::provider::error::CredentialsError;
use ref_sync_shared::errors::retryability::Retryability;

#[derive(Debug, thiserror::Error)]
pub enum RsS3PresignError {
    #[error("missing AWS credentials provider")]
    MissingCredentials,
    #[error("failed to load AWS credentials: {0}")]
    Credentials(#[from] CredentialsError),
    #[error("failed to upload object to S3: {0}")]
    Upload(#[source] reqwest::Error),
    #[error("S3 rejected upload for key {key:?} with status {status}: {body}")]
    UploadRejected {
        key: String,
        status: u16,
        body: String,
    },
    #[error("invalid S3 endpoint URL: {source}")]
    InvalidEndpoint {
        #[source]
        source: url::ParseError,
    },
    #[error("S3 endpoint {endpoint:?} does not contain a host")]
    InvalidEndpointHost { endpoint: String },
}

impl RsS3PresignError {
    pub fn retryability(&self) -> Retryability {
        match self {
            Self::MissingCredentials
            | Self::InvalidEndpoint { .. }
            | Self::InvalidEndpointHost { .. } => Retryability::Permanent,
            Self::Credentials(error) => Self::credentials_retryability(error),
            Self::Upload(_) => Retryability::Retryable,
            Self::UploadRejected { status, .. } => match status {
                408 | 429 => Retryability::Retryable,
                status if *status >= 500 => Retryability::Retryable,
                status if (400..500).contains(status) => Retryability::Permanent,
                _ => Retryability::Retryable,
            },
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
}
