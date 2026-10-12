use crate::{
    aws::Aws,
    dto::{
        event_converter::{EventConverter, event_converters},
        github::github_event_converter::GITHUB_WEBHOOK_SECRET,
    },
    error::{RsWebhookError, RsWebhookResult},
    infisical::secrets::Secrets,
};
use ref_sync_shared::error::ConfigError;

pub struct RsWebhookContext {
    pub aws: Aws,
    pub converters: Vec<Box<dyn EventConverter>>,
}

impl RsWebhookContext {
    pub async fn new() -> RsWebhookResult<Self> {
        let aws_timeouts = Aws::load_timeouts()?;
        let sdk_config = Aws::load_sdk_config(aws_timeouts).await;
        let aws = Aws::new(&sdk_config, aws_timeouts)?;
        let secrets = Secrets::load(&sdk_config).await?;
        let secret = secrets.get(GITHUB_WEBHOOK_SECRET)?;
        if secret.is_empty() {
            return Err(RsWebhookError::Config(ConfigError::empty(
                GITHUB_WEBHOOK_SECRET,
            )));
        }
        let converters = event_converters(secret);
        Ok(Self { aws, converters })
    }
}
