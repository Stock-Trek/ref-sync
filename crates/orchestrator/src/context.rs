use crate::{aws_ops::aws::Aws, error::RsOrchestratorResult};
use ref_sync_shared::git_ops::github::GitHubAuth;

pub struct GwhContext {
    pub aws: Aws,
    pub github: GitHubAuth,
}

impl GwhContext {
    pub async fn new() -> RsOrchestratorResult<Self> {
        let aws_timeouts = Aws::load_timeouts()?;
        let sdk_config = Aws::load_sdk_config(aws_timeouts).await;
        let aws = Aws::new(&sdk_config, aws_timeouts)?;
        let github = GitHubAuth::new()?;
        Ok(Self { aws, github })
    }
}
