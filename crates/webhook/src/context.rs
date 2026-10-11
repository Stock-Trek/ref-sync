use crate::{
    aws_ops::aws::Aws, dto::github::github_event_converter::GitHubWebhookConfig,
    error::RsWebhookResult, infisical::secrets::Secrets,
};
use ref_sync_shared::git_ops::github::GitHubAuth;

pub struct GwhContext {
    pub aws: Aws,
    pub github: GitHubAuth,
    pub webhook: GitHubWebhookConfig,
}

impl GwhContext {
    pub async fn new() -> RsWebhookResult<Self> {
        let aws_timeouts = Aws::load_timeouts()?;
        let sdk_config = Aws::load_sdk_config(aws_timeouts).await;
        let aws = Aws::new(&sdk_config, aws_timeouts)?;
        let github = GitHubAuth::new()?;
        let secrets = Secrets::load(&sdk_config).await?;
        let webhook = GitHubWebhookConfig::from_secrets(&secrets)?;
        Ok(Self {
            aws,
            github,
            webhook,
        })
    }
}
