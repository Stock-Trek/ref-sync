use crate::errors::{archive::RsArchiveError, error::RsOrchestratorResult};
use ref_sync_shared::git_ops::git_remote::GitFile;
use std::io::{Cursor, Write};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

const USER_GENERATED_PREFIX: &str = "runner/src/user_generated/";

pub struct SourceArchive;

impl SourceArchive {
    pub fn create(files: &[GitFile]) -> RsOrchestratorResult<Vec<u8>> {
        let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for file in files {
            archive
                .start_file(format!("{USER_GENERATED_PREFIX}{}", file.path), options)
                .map_err(RsArchiveError::from)?;
            archive
                .write_all(&file.contents)
                .map_err(RsArchiveError::from)?;
        }
        Ok(archive.finish().map_err(RsArchiveError::from)?.into_inner())
    }
}
