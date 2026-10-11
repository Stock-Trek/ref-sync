use crate::error::RsOrchestratorResult;
use async_trait::async_trait;
use std::time::SystemTime;

#[async_trait]
pub trait GitHost: Send + Sync {
    fn provider(&self) -> &'static str;

    fn installation_id(&self) -> String;

    async fn repo_ids(&self, deadline: SystemTime) -> RsOrchestratorResult<Vec<String>>;

    async fn repo_default_branch(&self, repo_id: &str, deadline: SystemTime) -> RsOrchestratorResult<String>;
}
