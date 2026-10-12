#[derive(Debug, thiserror::Error)]
pub enum RsTimeoutError {
    #[error("{operation} timed out")]
    Operation { operation: &'static str },
    #[error("deadline reached before operation could start")]
    DeadlineExceeded,
}
