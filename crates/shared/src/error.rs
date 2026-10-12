use aws_sdk_dynamodb::operation::update_item::UpdateItemError;
use aws_sdk_sqs::error::BuildError;
use aws_sdk_sqs::operation::send_message_batch::SendMessageBatchError;

pub type SharedResult<T> = Result<T, RefSyncSharedError>;

/// Whether an operation that failed may succeed if it is retried.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Retryability {
    /// The failure is expected to be transient; retrying may succeed.
    Retryable,
    /// The failure will not succeed unless something external changes.
    Permanent,
}

#[derive(Debug, thiserror::Error)]
pub enum RefSyncSharedError {
    #[error("Build: {0}")]
    Build(#[from] BuildError),
    #[error("Config: {0}")]
    Config(#[from] ConfigError),
    #[error("DynamoDbUpdateItem: {0}")]
    DynamoDbUpdateItem(Box<UpdateItemError>),
    #[error("EnvVar: {0}")]
    EnvVar(Box<std::env::VarError>),
    #[error("GitHub: {0}")]
    GitHub(#[from] GitHubError),
    #[error("InternalServer: {0}")]
    InternalServer(#[from] InternalServerError),
    #[error("InvalidMessage: {0}")]
    InvalidMessage(#[from] InvalidMessageError),
    #[error("Serde error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("SqsSendMessageBatch: {0}")]
    SqsSendMessageBatch(Box<SendMessageBatchError>),
    #[error("SqsSendMessageBatchPartial: {0}")]
    SqsSendMessageBatchPartial(SqsSendMessageBatchPartialError),
    #[error("Failed to set tracing subscriber: {0}")]
    SetTracingSubscriber(#[from] tracing::subscriber::SetGlobalDefaultError),
    #[error("Timeout: {0}")]
    Timeout(#[from] TimeoutError),
}

impl RefSyncSharedError {
    pub fn retryability(&self) -> Retryability {
        match self {
            Self::Build(_)
            | Self::Config(_)
            | Self::EnvVar(_)
            | Self::InvalidMessage(_)
            | Self::Serde(_)
            | Self::SetTracingSubscriber(_) => Retryability::Permanent,
            Self::GitHub(error) => error.retryability(),
            Self::DynamoDbUpdateItem(_)
            | Self::InternalServer(_)
            | Self::SqsSendMessageBatch(_)
            | Self::SqsSendMessageBatchPartial(_)
            | Self::Timeout(_) => Retryability::Retryable,
        }
    }

    pub fn is_retryable(&self) -> bool {
        self.retryability() == Retryability::Retryable
    }
}

/// Errors caused by missing, malformed, or otherwise invalid configuration.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// A required configuration value was not provided.
    #[error("{name} is missing")]
    Missing { name: String },
    /// A configuration value was provided but is empty.
    #[error("{name} must not be empty")]
    Empty { name: String },
    /// An environment variable was not valid UTF-8.
    #[error("{name} is not valid UTF-8")]
    NotUnicode { name: String },
    /// A configuration value failed to parse or validate.
    #[error("{name} is invalid: {message}")]
    Invalid {
        name: String,
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
    },
}

impl ConfigError {
    pub fn missing(name: impl Into<String>) -> Self {
        Self::Missing { name: name.into() }
    }

    pub fn empty(name: impl Into<String>) -> Self {
        Self::Empty { name: name.into() }
    }

    pub fn not_unicode(name: impl Into<String>) -> Self {
        Self::NotUnicode { name: name.into() }
    }

    pub fn invalid(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Invalid {
            name: name.into(),
            message: message.into(),
            source: None,
        }
    }

    pub fn invalid_source(
        name: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::Invalid {
            name: name.into(),
            message: source.to_string(),
            source: Some(Box::new(source)),
        }
    }
}

/// Errors emitted while talking to a git host.
#[derive(Debug, thiserror::Error)]
pub enum GitHubError {
    #[error("failed to build GitHub client: {0}")]
    BuildClient(#[source] octocrab::Error),
    #[error("failed to create client for installation {installation_id}: {source}")]
    Installation {
        installation_id: u64,
        #[source]
        source: octocrab::Error,
    },
    #[error("failed to list installation repositories: {0}")]
    ListRepositories(#[source] octocrab::Error),
    #[error("failed to get repository {repo_id}: {source}")]
    GetRepository {
        repo_id: u64,
        #[source]
        source: octocrab::Error,
    },
    #[error("repository {repo_id} was not found: {source}")]
    RepositoryNotFound {
        repo_id: u64,
        #[source]
        source: octocrab::Error,
    },
}

impl GitHubError {
    pub fn retryability(&self) -> Retryability {
        match self {
            Self::BuildClient(_) | Self::RepositoryNotFound { .. } => Retryability::Permanent,
            Self::Installation { source, .. }
            | Self::ListRepositories(source)
            | Self::GetRepository { source, .. } => octocrab_retryability(source),
        }
    }
}

fn octocrab_retryability(error: &octocrab::Error) -> Retryability {
    match error {
        octocrab::Error::GitHub { source, .. } => {
            let status = source.status_code.as_u16();
            if status == 403 || status == 408 || status == 429 || status >= 500 {
                Retryability::Retryable
            } else if (400..500).contains(&status) {
                Retryability::Permanent
            } else {
                Retryability::Retryable
            }
        }
        _ => Retryability::Retryable,
    }
}

/// Errors that indicate an unexpected server-side failure.
#[derive(Debug, thiserror::Error)]
pub enum InternalServerError {
    #[error("lock poisoned: {resource}")]
    LockPoisoned { resource: &'static str },
    #[error("DynamoDB update_item failed: {0}")]
    DynamoDbUpdateItem(#[source] Box<dyn std::error::Error + Send + Sync + 'static>),
}

/// Errors caused by a malformed inbound message.
#[derive(Debug, thiserror::Error)]
pub enum InvalidMessageError {
    #[error("GitHub repository id {repo_id:?} is not numeric: {source}")]
    GitHubRepositoryId {
        repo_id: String,
        #[source]
        source: std::num::ParseIntError,
    },
    #[error("invalid path: {value:?}")]
    InvalidPath { value: String },
    #[error("SQS record is missing a message id; cannot report a batch item failure")]
    MissingMessageId,
    #[error("SQS record is missing a body")]
    MissingBody,
}

/// Errors caused by an operation exceeding its deadline or timeout.
#[derive(Debug, thiserror::Error)]
pub enum TimeoutError {
    #[error("{operation} timed out")]
    Operation { operation: &'static str },
    #[error("deadline reached before operation could start")]
    DeadlineExceeded,
}

/// An SQS batch partially failed. Each failed entry is preserved individually.
#[derive(Debug, thiserror::Error)]
#[error("failed to send {} message(s): {}", .failures.len(), format_failures(.failures))]
pub struct SqsSendMessageBatchPartialError {
    pub failures: Vec<SqsBatchFailure>,
}

#[derive(Debug)]
pub struct SqsBatchFailure {
    pub id: String,
    pub code: String,
    pub message: Option<String>,
}

impl std::fmt::Display for SqsBatchFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "id={:?} code={:?} message={:?}",
            self.id, self.code, self.message
        )
    }
}

fn format_failures(failures: &[SqsBatchFailure]) -> String {
    failures
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}
