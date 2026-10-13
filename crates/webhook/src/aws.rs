use crate::error::RsWebhookResult;
use aws_config::{BehaviorVersion, SdkConfig, timeout::TimeoutConfig};
use ref_sync_shared::{sqs::Sqs, timeouts::Timeouts};
use std::time::Duration;

const DEFAULT_AWS_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const DEFAULT_AWS_OPERATION_TIMEOUT: Duration = Duration::from_secs(120);

pub struct Aws {
    pub sqs: Sqs,
}

impl Aws {
    pub fn load_timeouts() -> RsWebhookResult<Timeouts> {
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

    pub fn new(sdk_config: &SdkConfig, timeouts: Timeouts) -> RsWebhookResult<Self> {
        Ok(Self {
            sqs: Sqs::new(sdk_config, timeouts)?,
        })
    }
}
