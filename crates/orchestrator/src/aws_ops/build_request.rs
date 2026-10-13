use crate::{aws_ops::s3::PresignedPost, errors::error::RsOrchestratorResult};
use aws_sdk_ecs::types::{ContainerOverride, KeyValuePair};
use ref_sync_shared::sqs_message::{GitSource, SqsRefType};
use sha2::{Digest, Sha256};

const COMMIT_HASH: &str = "COMMIT_HASH";
const FORCED: &str = "FORCED";
const GIT_HOST: &str = "GIT_HOST";
const GIT_PROVIDER: &str = "GIT_PROVIDER";
const REPO_ID: &str = "REPO_ID";
const REF_NAME: &str = "REF_NAME";
const REF_TYPE: &str = "REF_TYPE";
const SIGNED_POST_FIELDS: &str = "SIGNED_POST_FIELDS";
const SIGNED_POST_URL: &str = "SIGNED_POST_URL";

pub struct BuildRequest {
    pub source: GitSource,
    pub repo_id: String,
    pub ref_name: String,
    pub ref_type: SqsRefType,
    pub commit_hash: Option<String>,
    pub forced: bool,
    pub signed_post: PresignedPost,
}

impl BuildRequest {
    pub fn client_token(&self) -> RsOrchestratorResult<String> {
        let stable = serde_json::to_vec(&(
            &self.source,
            &self.repo_id,
            &self.ref_name,
            self.ref_type,
            &self.commit_hash,
            self.forced,
            &self.signed_post,
        ))?;
        Ok(hex::encode(Sha256::digest(stable)))
    }
    pub fn container(&self, container_name: &str) -> RsOrchestratorResult<ContainerOverride> {
        let mut container = ContainerOverride::builder()
            .name(container_name)
            .environment(self.env_var(GIT_HOST, self.source.host()))
            .environment(self.env_var(GIT_PROVIDER, self.source.name()))
            .environment(self.env_var(REPO_ID, &self.repo_id))
            .environment(self.env_var(REF_NAME, &self.ref_name))
            .environment(self.env_var(REF_TYPE, &self.ref_type.to_string()))
            .environment(self.env_var(SIGNED_POST_URL, &self.signed_post.url))
            .environment(self.env_var(SIGNED_POST_FIELDS, &self.signed_post.fields_json()?));
        if let Some(commit_hash) = self.commit_hash.as_deref() {
            container = container
                .environment(self.env_var(COMMIT_HASH, commit_hash))
                .environment(self.env_var(FORCED, &self.forced.to_string()));
        }
        Ok(container.build())
    }
    fn env_var(&self, name: &str, value: &str) -> KeyValuePair {
        KeyValuePair::builder().name(name).value(value).build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn post(url: &str, signature: &str) -> PresignedPost {
        PresignedPost {
            url: url.to_string(),
            fields: BTreeMap::from([
                ("key".to_string(), "key/${filename}".to_string()),
                ("x-amz-signature".to_string(), signature.to_string()),
            ]),
        }
    }

    fn request(signed_post: PresignedPost) -> BuildRequest {
        BuildRequest {
            source: GitSource::GitHub {
                host: "github.com".to_string(),
                delivery_id: None,
                installation_id: 1,
            },
            repo_id: "123".to_string(),
            ref_name: "main".to_string(),
            ref_type: SqsRefType::Branch,
            commit_hash: Some("deadbeef".to_string()),
            forced: false,
            signed_post,
        }
    }

    #[test]
    fn client_token_is_stable_for_identical_requests() {
        let first = request(post("https://example.com/bucket", "sig-1"))
            .client_token()
            .unwrap();
        let second = request(post("https://example.com/bucket", "sig-1"))
            .client_token()
            .unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn client_token_changes_with_signed_post_fields() {
        let first = request(post("https://example.com/bucket", "sig-1"))
            .client_token()
            .unwrap();
        let second = request(post("https://example.com/bucket", "sig-2"))
            .client_token()
            .unwrap();
        assert_ne!(first, second);
    }

    #[test]
    fn client_token_changes_with_signed_post_url() {
        let first = request(post("https://example.com/bucket", "sig-1"))
            .client_token()
            .unwrap();
        let second = request(post("https://example.com/other", "sig-1"))
            .client_token()
            .unwrap();
        assert_ne!(first, second);
    }
}
