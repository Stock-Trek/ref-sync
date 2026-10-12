use crate::{error::SharedResult, timeouts::Deadline};
use async_trait::async_trait;

#[async_trait]
pub trait GitHost: Send + Sync {
    fn provider(&self) -> &'static str;

    fn host(&self) -> &str;

    fn installation_id(&self) -> String;

    async fn repo_ids(&self, deadline: Deadline) -> SharedResult<Vec<String>>;

    async fn repo_default_branch(&self, repo_id: &str, deadline: Deadline) -> SharedResult<String>;
}
