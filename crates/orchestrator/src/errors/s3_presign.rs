use aws_credential_types::provider::error::CredentialsError;
use ref_sync_shared::errors::retryability::Retryability;

#[derive(Debug, thiserror::Error)]
pub enum RsS3PresignError {
    #[error("missing AWS credentials provider")]
    MissingCredentials,
    #[error("failed to load AWS credentials: {0}")]
    Credentials(#[from] CredentialsError),
}

impl RsS3PresignError {
    pub fn retryability(&self) -> Retryability {
        match self {
            Self::MissingCredentials => Retryability::Permanent,
            Self::Credentials(error) => Self::credentials_retryability(error),
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
