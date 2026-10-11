use crate::error::GwhResult;
use async_trait::async_trait;
use std::time::SystemTime;

#[async_trait]
pub trait GitHost: Send + Sync {
    fn provider(&self) -> &'static str;

    fn installation_id(&self) -> String;

    async fn repo_ids(&self, deadline: SystemTime) -> GwhResult<Vec<String>>;

    async fn repo_default_branch(&self, repo_id: &str, deadline: SystemTime) -> GwhResult<String>;
}
