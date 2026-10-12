use serde::{Deserialize, Serialize};
use strum::Display;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SqsMessage {
    pub source: GitSource,
    pub detail: SqsDetail,
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
    Transfer,
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
        host: String,
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
