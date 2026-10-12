use crate::error::SharedResult;
use async_trait::async_trait;

/// A git host bound to the deadline of the request it was created for.
///
/// The deadline is supplied when the host is created rather than on each
/// method, so callers do not have to thread and handle it themselves.
#[async_trait]
pub trait GitHost: Send + Sync {
    fn provider(&self) -> &'static str;

    fn host(&self) -> &str;

    fn installation_id(&self) -> String;

    async fn repo_ids(&self) -> SharedResult<Vec<String>>;

    async fn repo_default_branch(&self, repo_id: &str) -> SharedResult<String>;
}
