use ref_sync_shared::errors::retryability::Retryability;

#[derive(Debug, thiserror::Error)]
pub enum RsArchiveError {
    #[error("failed to write source archive: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to create source archive: {0}")]
    Zip(#[from] zip::result::ZipError),
}

impl RsArchiveError {
    pub fn retryability(&self) -> Retryability {
        Retryability::Permanent
    }
}
