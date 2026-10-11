pub type SharedResult<T> = Result<T, SharedError>;

#[derive(Debug, thiserror::Error)]
pub enum SharedError {
    #[error("Config: {0}")]
    Config(String),
    #[error("EnvVar: {0}")]
    EnvVar(Box<std::env::VarError>),
    #[error("Serde error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("Failed to set tracing subscriber: {0}")]
    SetTracingSubscriber(#[from] tracing::subscriber::SetGlobalDefaultError),
    #[error("Timeout: {0}")]
    Timeout(String),
}
