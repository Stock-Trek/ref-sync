use crate::{aws_ops::dynamodb::DynamoDb, error::GwhResult, timeouts::Timeouts};
use aws_config::{BehaviorVersion, SdkConfig, timeout::TimeoutConfig};
use std::time::Duration;

const DEFAULT_AWS_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const DEFAULT_AWS_OPERATION_TIMEOUT: Duration = Duration::from_secs(120);

pub struct Aws {
    pub dynamodb: DynamoDb,
}

impl Aws {
    pub fn load_timeouts() -> GwhResult<Timeouts> {
        Timeouts::from_env(
            "AWS",
            DEFAULT_AWS_CONNECT_TIMEOUT,
            DEFAULT_AWS_OPERATION_TIMEOUT,
        )
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

    pub fn new(sdk_config: &SdkConfig, timeouts: Timeouts) -> GwhResult<Self> {
        Ok(Self {
            dynamodb: DynamoDb::new(sdk_config, timeouts)?,
        })
    }
}
