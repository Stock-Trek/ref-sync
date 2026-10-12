use serde::{Deserialize, Serialize};
use strum::Display;

#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum TombstoneProvider {
    GitHub,
    GitLab,
    Bitbucket,
    Gitea,
    Forgejo,
    Generic,
}

#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum TombstoneScopeType {
    Installation,
    Workspace,
    Organization,
    Group,
    User,
    App,
    None,
}

#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum TombstoneReason {
    ScopeDeleted,
    ScopeRevoked,
    RepoDeleted,
    RepoDetached,
    RepoTransferred,
    RefDeleted,
    Reconciled,
}

#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum TombstoneDetectedVia {
    Webhook,
    Poll,
    Reconcile,
    Manual,
}

#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum RefType {
    Branch,
    Tag,
    MergeRequest,
    Note,
    Other,
}

#[derive(Clone)]
pub struct EnclosingScope {
    pub scope_type: TombstoneScopeType,
    pub scope_id: String,
}
