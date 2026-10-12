use crate::{
    aws::Aws,
    dto::{event_converter::EventConverter, github::github_event_converter::GitHubWebhookConfig},
    error::RsWebhookResult,
    infisical::secrets::Secrets,
    provider,
};

pub struct GwhContext {
    pub aws: Aws,
    pub converters: Vec<Box<dyn EventConverter>>,
}

impl GwhContext {
    pub async fn new() -> RsWebhookResult<Self> {
        let aws_timeouts = Aws::load_timeouts()?;
        let sdk_config = Aws::load_sdk_config(aws_timeouts).await;
        let aws = Aws::new(&sdk_config, aws_timeouts)?;
        let secrets = Secrets::load(&sdk_config).await?;
        let github = GitHubWebhookConfig::from_secrets(&secrets)?;
        let converters = provider::event_converters(github);
        Ok(Self { aws, converters })
    }
}
