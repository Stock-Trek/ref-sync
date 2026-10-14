use crate::errors::error::RsOrchestratorResult;
use ref_sync_shared::{
    errors::{error::RsSharedError, invalid_message::RsInvalidMessageError},
    git_ops::git_remote::GitHost,
    sqs_message::SqsRefType,
};
use std::fmt::Display;

pub struct S3Keys;

impl S3Keys {
    pub fn ref_key<InstallationId, RepoId, Remote>(
        git_remote: &Remote,
        repo_id: RepoId,
        ref_name: &str,
        ref_type: SqsRefType,
        commit_hash: &str,
    ) -> RsOrchestratorResult<String>
    where
        InstallationId: Display,
        RepoId: Display,
        Remote: GitHost<InstallationId, RepoId>,
    {
        let repo = Self::sanitize_path(&repo_id.to_string())?;
        let ref_name = Self::sanitize_path(ref_name)?;
        let commit_hash = Self::sanitize_path(commit_hash)?;
        let ref_type_name = match ref_type {
            SqsRefType::Branch => "heads",
            SqsRefType::Tag => "tags",
        };
        Ok(format!(
            "{}/{}/{}/{}/{}/{}/{}/",
            git_remote.provider(),
            git_remote.host(),
            git_remote.installation_id(),
            repo,
            ref_type_name,
            ref_name,
            commit_hash
        ))
    }
}

impl S3Keys {
    fn sanitize_path(value: &str) -> RsOrchestratorResult<String> {
        if Self::is_valid_ref_name(value) {
            Ok(value.to_string())
        } else {
            Err(RsSharedError::InvalidMessage(
                RsInvalidMessageError::InvalidPath {
                    value: value.to_string(),
                },
            ))?
        }
    }

    fn is_valid_ref_name(value: &str) -> bool {
        !value.is_empty()
            && value != "@"
            && !value.starts_with('/')
            && !value.ends_with('/')
            && !value.ends_with('.')
            && !value.contains("//")
            && !value.contains("..")
            && !value.contains("@{")
            && value.split('/').all(|segment| {
                !segment.is_empty() && !segment.starts_with('.') && !segment.ends_with(".lock")
            })
            && value.chars().all(|character| {
                !character.is_ascii_control()
                    && character != ' '
                    && !matches!(character, '~' | '^' | ':' | '?' | '*' | '[' | '\\')
            })
    }
}
