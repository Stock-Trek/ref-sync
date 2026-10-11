use crate::{
    error::GwhResult,
    sqs_message::{GitSource, SqsRefType},
};
use aws_sdk_ecs::types::{ContainerOverride, KeyValuePair};
use sha2::{Digest, Sha256};

const COMMIT_HASH: &str = "COMMIT_HASH";
const FORCED: &str = "FORCED";
const GIT_PROVIDER: &str = "GIT_PROVIDER";
const REPO_ID: &str = "REPO_ID";
const REF_NAME: &str = "REF_NAME";
const REF_TYPE: &str = "REF_TYPE";
const SIGNED_PUT_URL: &str = "SIGNED_PUT_URL";

pub struct BuildRequest {
    pub source: GitSource,
    pub repo_id: String,
    pub ref_name: String,
    pub ref_type: SqsRefType,
    pub commit_hash: Option<String>,
    pub forced: bool,
    pub signed_s3_put_url: String,
}

impl BuildRequest {
    pub fn client_token(&self) -> GwhResult<String> {
        let stable = serde_json::to_vec(&(
            &self.source,
            &self.repo_id,
            &self.ref_name,
            self.ref_type,
            &self.commit_hash,
            self.forced,
        ))?;
        Ok(hex::encode(Sha256::digest(stable)))
    }
    pub fn container(&self, container_name: &str) -> ContainerOverride {
        let mut container = ContainerOverride::builder()
            .name(container_name)
            .environment(self.env_var(GIT_PROVIDER, self.source.name()))
            .environment(self.env_var(REPO_ID, &self.repo_id))
            .environment(self.env_var(REF_NAME, &self.ref_name))
            .environment(self.env_var(REF_TYPE, &self.ref_type.to_string()))
            .environment(self.env_var(SIGNED_PUT_URL, &self.signed_s3_put_url));
        if let Some(commit_hash) = self.commit_hash.as_deref() {
            container = container
                .environment(self.env_var(COMMIT_HASH, commit_hash))
                .environment(self.env_var(FORCED, &self.forced.to_string()));
        }
        container.build()
    }
    fn env_var(&self, name: &str, value: &str) -> KeyValuePair {
        KeyValuePair::builder().name(name).value(value).build()
    }
}
