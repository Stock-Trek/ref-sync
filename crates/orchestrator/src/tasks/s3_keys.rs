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

    pub fn source_key<InstallationId, RepoId, Remote>(
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
        Ok(format!(
            "{}source/algorithms.zip",
            Self::ref_key(git_remote, repo_id, ref_name, ref_type, commit_hash)?
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

#[cfg(test)]
mod tests {
    use super::*;
    use ref_sync_shared::{
        deadline::Deadline, errors::error::RsSharedResult, git_ops::git_remote::GitFile,
    };

    struct TestRemote;

    impl GitHost<u64, u64> for TestRemote {
        fn provider(&self) -> &'static str {
            "github"
        }

        fn host(&self) -> &str {
            "github.com"
        }

        fn installation_id(&self) -> u64 {
            42
        }

        fn parse_repo_id(&self, repo_id: &str) -> RsSharedResult<u64> {
            Ok(repo_id.parse().unwrap())
        }

        async fn repo_ids(&self, _deadline: Deadline) -> RsSharedResult<Vec<u64>> {
            unimplemented!()
        }

        async fn repo_default_branch(
            &self,
            _repo_id: u64,
            _deadline: Deadline,
        ) -> RsSharedResult<String> {
            unimplemented!()
        }

        async fn ref_commit(
            &self,
            _repo_id: u64,
            _ref_name: &str,
            _deadline: Deadline,
        ) -> RsSharedResult<String> {
            unimplemented!()
        }

        async fn directory_files(
            &self,
            _repo_id: u64,
            _ref_name: &str,
            _directory: &str,
            _deadline: Deadline,
        ) -> RsSharedResult<Vec<GitFile>> {
            unimplemented!()
        }
    }

    #[test]
    fn source_archive_is_stored_below_the_ref_prefix() {
        let key = S3Keys::source_key(&TestRemote, 7, "main", SqsRefType::Branch, "abc123").unwrap();
        assert_eq!(
            key,
            "github/github.com/42/7/heads/main/abc123/source/algorithms.zip"
        );
    }
}
