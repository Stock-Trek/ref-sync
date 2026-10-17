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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use zip::ZipArchive;

    #[test]
    fn creates_archive_for_build_agent_user_generated_directory() {
        let files = vec![
            GitFile {
                path: "algorithms.rs".to_string(),
                contents: b"pub fn run() {}".to_vec(),
            },
            GitFile {
                path: "nested/custom.rs".to_string(),
                contents: b"const VALUE: u32 = 1;".to_vec(),
            },
        ];

        let archive = SourceArchive::create(&files).unwrap();
        let mut archive = ZipArchive::new(Cursor::new(archive)).unwrap();

        {
            let mut algorithms = archive
                .by_name("runner/src/user_generated/algorithms.rs")
                .unwrap();
            let mut contents = String::new();
            algorithms.read_to_string(&mut contents).unwrap();
            assert_eq!(contents, "pub fn run() {}");
        }

        {
            let mut custom = archive
                .by_name("runner/src/user_generated/nested/custom.rs")
                .unwrap();
            let mut contents = String::new();
            custom.read_to_string(&mut contents).unwrap();
            assert_eq!(contents, "const VALUE: u32 = 1;");
        }
    }
}
