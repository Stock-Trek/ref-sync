use crate::error::{RsOrchestratorError, RsOrchestratorResult};
use ref_sync_shared::{
    error::InvalidMessageError, git_ops::git_remote::GitRemote, sqs_message::SqsRefType,
};

pub struct S3Keys;

impl S3Keys {
    pub fn ref_key(
        git_remote: &GitRemote,
        repo_id: &str,
        ref_name: &str,
        ref_type: SqsRefType,
    ) -> RsOrchestratorResult<String> {
        let repo = Self::sanitize_path(repo_id)?;
        let ref_name = Self::sanitize_path(ref_name)?;
        let ref_type_name = match ref_type {
            SqsRefType::Branch => "heads",
            SqsRefType::Tag => "tags",
        };
        Ok(format!(
            "{}/{}/{}/{}/{}/{}/",
            git_remote.provider(),
            git_remote.host(),
            git_remote.installation_id(),
            repo,
            ref_type_name,
            ref_name
        ))
    }
}

impl S3Keys {
    fn sanitize_path(value: &str) -> RsOrchestratorResult<String> {
        if Self::is_valid_ref_name(value) {
            Ok(value.to_string())
        } else {
            Err(RsOrchestratorError::InvalidMessage(
                InvalidMessageError::InvalidPath {
                    value: value.to_string(),
                },
            ))
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
