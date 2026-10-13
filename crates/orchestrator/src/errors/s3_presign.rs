use aws_credential_types::provider::error::CredentialsError;

#[derive(Debug, thiserror::Error)]
pub enum RsS3PresignError {
    #[error("missing AWS credentials provider")]
    MissingCredentials,
    #[error("failed to load AWS credentials: {0}")]
    Credentials(#[from] CredentialsError),
}
