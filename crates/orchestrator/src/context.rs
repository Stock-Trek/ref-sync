use crate::{aws_ops::aws::Aws, errors::error::RsOrchestratorResult};
use ref_sync_shared::git_ops::github::github_auth::GitHubAuth;

pub struct RsOrchestratorContext {
    pub aws: Aws,
    pub github_auth: GitHubAuth,
}

impl RsOrchestratorContext {
    pub async fn new() -> RsOrchestratorResult<Self> {
        let aws_timeouts = Aws::load_timeouts()?;
        let sdk_config = Aws::load_sdk_config(aws_timeouts).await;
        let aws = Aws::new(&sdk_config, aws_timeouts)?;
        let github_auth = GitHubAuth::new()?;
        Ok(Self { aws, github_auth })
    }
}
