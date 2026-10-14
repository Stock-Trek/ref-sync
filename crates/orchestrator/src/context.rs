use crate::{aws_ops::aws::Aws, errors::error::RsOrchestratorResult};
use ref_sync_shared::{env_vars::EnvVarConfig, git_ops::github::github_auth::GitHubAuth};
use std::time::Duration;

pub struct RsOrchestratorContext {
    pub aws: Aws,
    pub github_auth: GitHubAuth,
    pub sqs_process_concurrency: usize,
    pub deadline_buffer: Duration,
}

impl RsOrchestratorContext {
    pub async fn new(config: &EnvVarConfig) -> RsOrchestratorResult<Self> {
        let aws_timeouts = Aws::load_timeouts(config)?;
        let sdk_config = Aws::load_sdk_config(aws_timeouts).await;
        let aws = Aws::from_env(&sdk_config, aws_timeouts, config)?;
        let github_auth = GitHubAuth::from_env(config)?;
        let sqs_process_concurrency = config.sqs_process_concurrency()?.get();
        let deadline_buffer = config.deadline_buffer()?;
        Ok(Self {
            aws,
            github_auth,
            sqs_process_concurrency,
            deadline_buffer,
        })
    }
}
