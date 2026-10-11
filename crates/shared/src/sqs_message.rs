use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use strum::Display;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SqsMessage {
    pub source: GitSource,
    pub detail: SqsDetail,
}

impl SqsMessage {
    pub fn idempotency_key(&self) -> Option<String> {
        let delivery_id = self.source.delivery_id()?;
        let detail = serde_json::to_vec(&self.detail).ok()?;
        let digest = hex::encode(Sha256::digest(&detail));
        Some(format!("github/{delivery_id}/{digest}"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SqsDetail {
    AllRepos {
        action: SqsMultiRepoAction,
    },
    MultiRepo {
        action: SqsMultiRepoAction,
        repo_ids: Vec<String>,
    },
    Repo {
        action: SqsRepoAction,
        repo_id: String,
    },
    Ref {
        action: SqsRefAction,
        repo_id: String,
        ref_name: String,
        ref_type: SqsRefType,
    },
    Commit {
        repo_id: String,
        branch_name: String,
        commit_hash: String,
        forced: bool,
    },
}

#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum SqsMultiRepoAction {
    Add,
    Remove,
}

#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum SqsRepoAction {
    Add,
    Remove,
}

#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum SqsRefAction {
    Create,
    Delete,
}

#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum SqsRefType {
    Branch,
    Tag,
}

#[derive(Debug, Display, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum GitSource {
    GitHub {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        delivery_id: Option<String>,
        installation_id: u64,
    },
}

impl GitSource {
    pub fn name(&self) -> &'static str {
        match self {
            GitSource::GitHub { .. } => "github",
        }
    }
    pub fn delivery_id(&self) -> Option<&str> {
        match self {
            GitSource::GitHub { delivery_id, .. } => delivery_id.as_deref(),
        }
    }
}
