#[derive(Debug, thiserror::Error)]
pub enum RsInvalidMessageError {
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
