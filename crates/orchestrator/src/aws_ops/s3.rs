use crate::error::{RsOrchestratorError, RsOrchestratorResult};
use aws_config::SdkConfig;
use aws_sdk_s3::{Client as S3Client, presigning::PresigningConfig};
use ref_sync_shared::env_vars::EnvVars;
use std::time::Duration;

const S3_BUCKET_USER_ARTIFACTS: &str = "S3_BUCKET_USER_ARTIFACTS";

pub struct S3Config {
    pub user_artifacts_bucket: String,
}

impl S3Config {
    pub fn from_env() -> RsOrchestratorResult<Self> {
        Ok(Self {
            user_artifacts_bucket: EnvVars::env_var(S3_BUCKET_USER_ARTIFACTS)?,
        })
    }
}

pub struct S3 {
    client: S3Client,
    config: S3Config,
}

impl S3 {
    pub fn new(sdk_config: &SdkConfig) -> RsOrchestratorResult<Self> {
        Ok(Self {
            client: S3Client::new(sdk_config),
            config: S3Config::from_env()?,
        })
    }

    pub async fn signed_put_url(
        &self,
        key: &str,
        expires_in: Duration,
    ) -> RsOrchestratorResult<String> {
        let presigning_config = PresigningConfig::expires_in(expires_in)
            .map_err(|error| RsOrchestratorError::S3Presign(error.to_string()))?;
        let presigned = self
            .client
            .put_object()
            .bucket(&self.config.user_artifacts_bucket)
            .key(key)
            .presigned(presigning_config)
            .await
            .map_err(|error| RsOrchestratorError::S3Presign(error.to_string()))?;
        Ok(presigned.uri().to_string())
    }
}
