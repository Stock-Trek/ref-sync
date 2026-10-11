use crate::{
    aws_ops::{dynamodb::DynamoDb, ecs::Ecs, s3::S3},
    error::RsOrchestratorResult,
};
use aws_config::{BehaviorVersion, SdkConfig, timeout::TimeoutConfig};
use ref_sync_shared::timeouts::Timeouts;
use std::time::Duration;

const DEFAULT_AWS_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const DEFAULT_AWS_OPERATION_TIMEOUT: Duration = Duration::from_secs(120);

pub struct Aws {
    pub dynamodb: DynamoDb,
    pub ecs: Ecs,
    pub s3: S3,
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
        let timeout_config = TimeoutConfig::builder()
            .connect_timeout(timeouts.connect)
            .operation_timeout(timeouts.operation)
            .build();
        aws_config::defaults(BehaviorVersion::latest())
            .timeout_config(timeout_config)
            .load()
            .await
    }

    pub fn new(sdk_config: &SdkConfig, timeouts: Timeouts) -> RsOrchestratorResult<Self> {
        Ok(Self {
            dynamodb: DynamoDb::new(sdk_config, timeouts)?,
            ecs: Ecs::new(sdk_config, timeouts)?,
            s3: S3::new(sdk_config, timeouts)?,
        })
    }
}
