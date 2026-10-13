use crate::{
    aws_ops::aws::Aws,
    constants::{DEFAULT_SQS_PROCESS_CONCURRENCY, SQS_PROCESS_CONCURRENCY_KEY},
    errors::error::RsOrchestratorResult,
};
use ref_sync_shared::{
    env_vars::EnvVars,
    errors::{config::RsConfigError, error::RsSharedError},
    git_ops::github::github_auth::GitHubAuth,
};
use std::num::NonZeroUsize;

pub struct RsOrchestratorContext {
    pub aws: Aws,
    pub github_auth: GitHubAuth,
    pub sqs_process_concurrency: usize,
}

impl RsOrchestratorContext {
    pub async fn new() -> RsOrchestratorResult<Self> {
        let aws_timeouts = Aws::load_timeouts()?;
        let sdk_config = Aws::load_sdk_config(aws_timeouts).await;
        let aws = Aws::new(&sdk_config, aws_timeouts)?;
        let github_auth = GitHubAuth::new()?;
        let sqs_process_concurrency = NonZeroUsize::new(EnvVars::usize(
            SQS_PROCESS_CONCURRENCY_KEY,
            DEFAULT_SQS_PROCESS_CONCURRENCY,
        )?)
        .ok_or_else(|| {
            RsSharedError::Config(RsConfigError::invalid(
                SQS_PROCESS_CONCURRENCY_KEY,
                "must be greater than zero",
            ))
        })?
        .get();
        Ok(Self {
            aws,
            github_auth,
            sqs_process_concurrency,
        })
    }
}
