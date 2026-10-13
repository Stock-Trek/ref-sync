use crate::{
    aws_ops::{ecs::Ecs, s3::S3},
    errors::error::RsOrchestratorResult,
};
use aws_config::{BehaviorVersion, SdkConfig, timeout::TimeoutConfig};
use ref_sync_shared::{sqs::Sqs, timeouts::Timeouts};
use std::time::Duration;

const DEFAULT_AWS_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const DEFAULT_AWS_OPERATION_TIMEOUT: Duration = Duration::from_secs(120);

pub struct Aws {
    pub ecs: Ecs,
    pub s3: S3,
    pub sqs: Sqs,
}

impl Aws {
    pub fn load_timeouts() -> RsOrchestratorResult<Timeouts> {
        Ok(Timeouts::from_env(
            "AWS",
            DEFAULT_AWS_CONNECT_TIMEOUT,
            DEFAULT_AWS_OPERATION_TIMEOUT,
        )?)
    }
    pub async fn load_sdk_config(timeouts: Timeouts) -> SdkConfig {
        let mut timeout_config = TimeoutConfig::builder();
        timeout_config
            .set_connect_timeout(timeouts.connect)
            .set_operation_timeout(timeouts.operation);
        let timeout_config = timeout_config.build();
        aws_config::defaults(BehaviorVersion::latest())
            .timeout_config(timeout_config)
            .load()
            .await
    }

    pub fn new(ecs: Ecs, s3: S3, sqs: Sqs) -> Self {
        Self { ecs, s3, sqs }
    }

    pub fn from_env(sdk_config: &SdkConfig, timeouts: Timeouts) -> RsOrchestratorResult<Self> {
        Ok(Self::new(
            Ecs::from_env(sdk_config, timeouts)?,
            S3::from_env(sdk_config, timeouts)?,
            Sqs::from_env(sdk_config, timeouts)?,
        ))
    }
}
