use crate::{
    aws::Aws,
    dto::event_converter::{EventConverter, event_converters},
    error::RsWebhookResult,
    infisical::secrets::Secrets,
};

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
        let converters = event_converters(&secrets)?;
        Ok(Self { aws, converters })
    }
}
