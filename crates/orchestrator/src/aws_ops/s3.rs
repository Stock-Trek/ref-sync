use crate::{
    env_vars::EnvVars,
    error::{RsOrchestratorError, RsOrchestratorResult},
    timeouts::Timeouts,
};
use aws_config::SdkConfig;
use aws_sdk_s3::{
    Client as S3Client,
    presigning::PresigningConfig,
    types::{Delete, ObjectIdentifier},
};
use std::time::{Duration, SystemTime};

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
    timeouts: Timeouts,
}

impl S3 {
    pub fn new(sdk_config: &SdkConfig, timeouts: Timeouts) -> RsOrchestratorResult<Self> {
        Ok(Self {
            client: S3Client::new(sdk_config),
            config: S3Config::from_env()?,
            timeouts,
        })
    }

    pub async fn signed_put_url(&self, key: &str, expires_in: Duration) -> RsOrchestratorResult<String> {
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

    pub async fn delete(&self, key: &str, deadline: SystemTime) -> RsOrchestratorResult<()> {
        let timeout = self.timeouts.operation_for(deadline)?;
        tokio::time::timeout(timeout, async {
            self.client
                .delete_object()
                .bucket(&self.config.user_artifacts_bucket)
                .key(key)
                .send()
                .await
                .map_err(|e| RsOrchestratorError::S3DeleteObject(Box::new(e.into_service_error())))?;
            Ok(())
        })
        .await
        .map_err(|_| RsOrchestratorError::Timeout("delete timed out".into()))?
    }

    pub async fn delete_objects_with_prefix(
        &self,
        prefix: &str,
        deadline: SystemTime,
    ) -> RsOrchestratorResult<()> {
        self.timeouts
            .with_deadline(
                deadline,
                "S3 delete_objects",
                self.delete_objects_with_prefix_inner(prefix),
            )
            .await?
    }

    pub async fn list_keys_with_prefix(
        &self,
        prefix: &str,
        deadline: SystemTime,
    ) -> RsOrchestratorResult<Vec<String>> {
        self.timeouts
            .with_deadline(
                deadline,
                "S3 list_objects",
                self.list_keys_with_prefix_inner(prefix),
            )
            .await?
    }

    async fn list_keys_with_prefix_inner(&self, prefix: &str) -> RsOrchestratorResult<Vec<String>> {
        let mut paginator = self
            .client
            .list_objects_v2()
            .bucket(&self.config.user_artifacts_bucket)
            .prefix(prefix)
            .into_paginator()
            .send();
        let mut keys = Vec::new();
        while let Some(page) = paginator.next().await {
            let page =
                page.map_err(|e| RsOrchestratorError::S3ListObjects(Box::new(e.into_service_error())))?;
            for object in page.contents() {
                if let Some(key) = object.key() {
                    keys.push(key.to_string());
                }
            }
        }
        Ok(keys)
    }

    async fn delete_objects_with_prefix_inner(&self, prefix: &str) -> RsOrchestratorResult<()> {
        let keys = self.list_keys_with_prefix_inner(prefix).await?;
        if keys.is_empty() {
            return Ok(());
        }
        let objects_to_delete = keys
            .into_iter()
            .map(|key| {
                ObjectIdentifier::builder()
                    .key(key)
                    .build()
                    .map_err(RsOrchestratorError::Build)
            })
            .collect::<RsOrchestratorResult<Vec<_>>>()?;
        for chunk in objects_to_delete.chunks(1000) {
            let delete = Delete::builder()
                .set_objects(Some(chunk.to_vec()))
                .build()
                .map_err(RsOrchestratorError::Build)?;
            let output = self
                .client
                .delete_objects()
                .bucket(&self.config.user_artifacts_bucket)
                .delete(delete)
                .send()
                .await
                .map_err(|e| RsOrchestratorError::S3DeleteObjects(Box::new(e.into_service_error())))?;
            let errors = output.errors();
            if !errors.is_empty() {
                let failures = errors
                    .iter()
                    .map(|failure| {
                        format!(
                            "key={:?} code={:?} message={:?}",
                            failure.key(),
                            failure.code(),
                            failure.message()
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(RsOrchestratorError::S3DeleteObjectsPartial(format!(
                    "Failed to delete {} object(s) under {}/{}: {failures}",
                    errors.len(),
                    self.config.user_artifacts_bucket,
                    prefix
                )));
            }
        }
        Ok(())
    }
}
