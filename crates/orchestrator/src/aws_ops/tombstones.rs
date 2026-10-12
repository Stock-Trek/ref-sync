use std::{
    collections::HashMap,
    time::{SystemTime, UNIX_EPOCH},
};

use aws_sdk_dynamodb::types::AttributeValue;
use ref_sync_shared::sqs_message::GitSource;
use serde::{Deserialize, Serialize};
use strum::Display;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Tombstone {
    Scope {
        #[serde(flatten)]
        common: TombstoneCommon,
    },
    Repository {
        #[serde(flatten)]
        common: TombstoneCommon,
        repo_id: String,
    },
    Ref {
        #[serde(flatten)]
        common: TombstoneCommon,
        repo_id: String,
        ref_name: String,
        ref_type: RefType,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TombstoneCommon {
    pk: String,
    sk: String,

    provider: Provider,
    host: String,
    reason: Reason,
    deleted_at_millis: u64,
    last_seen_at_millis: u64,
    detected_via: DetectedVia,
    delivery_count: u32,

    #[serde(skip_serializing_if = "Option::is_none")]
    scope_type: Option<ScopeType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scope_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scope_pk: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scope_sk: Option<String>,
    audit_pk: String,
    audit_sk: String,
}

#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum Provider {
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
pub enum ScopeType {
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
pub enum Reason {
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
pub enum DetectedVia {
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

pub struct TombstoneContext {
    pub provider: Provider,
    pub host: String,
    pub reason: Reason,
    pub detected_via: DetectedVia,
    pub deleted_at_millis: u64,
    pub scope: Option<EnclosingScope>,
}

#[derive(Clone)]
pub struct EnclosingScope {
    pub scope_type: ScopeType,
    pub scope_id: String,
}

impl Tombstone {
    pub fn scope(ctx: &TombstoneContext, scope_type: ScopeType, scope_id: String) -> Self {
        let pk = format!("HOST#{}#SCOPE#{}", ctx.host, scope_id);
        let sk = "DEL#SCOPE".to_string();
        let common = TombstoneCommon::build(
            pk,
            sk,
            ctx,
            "SCOPE",
            &scope_id.clone(),
            Some(EnclosingScope {
                scope_type,
                scope_id,
            }),
        );
        Tombstone::Scope { common }
    }

    pub fn repository(ctx: &TombstoneContext, repo_id: String) -> Self {
        let pk = format!("HOST#{}#REPO#{}", ctx.host, repo_id);
        let sk = "DEL#REPOSITORY".to_string();
        let common = TombstoneCommon::build(pk, sk, ctx, "REPOSITORY", &repo_id, ctx.scope.clone());
        Tombstone::Repository { common, repo_id }
    }

    pub fn reference(
        ctx: &TombstoneContext,
        repo_id: String,
        ref_name: String,
        ref_type: RefType,
    ) -> Self {
        let pk = format!("HOST#{}#REPO#{}", ctx.host, repo_id);
        let sk = format!("DEL#REF#{}", ref_name);
        let natural_id = format!("{}#{}", repo_id, ref_name);
        let common = TombstoneCommon::build(pk, sk, ctx, "REF", &natural_id, ctx.scope.clone());
        Tombstone::Ref {
            common,
            repo_id,
            ref_name,
            ref_type,
        }
    }
    pub fn attributes(&self) -> Vec<(String, AttributeValue)> {
        let mut attributes = self.common().attributes();
        match self {
            Tombstone::Scope { .. } => {}
            Tombstone::Repository { repo_id, .. } => {
                attributes.push(("repo_id".to_string(), AttributeValue::S(repo_id.clone())));
            }
            Tombstone::Ref {
                repo_id,
                ref_name,
                ref_type,
                ..
            } => {
                attributes.push(("repo_id".to_string(), AttributeValue::S(repo_id.clone())));
                attributes.push(("ref_name".to_string(), AttributeValue::S(ref_name.clone())));
                attributes.push((
                    "ref_type".to_string(),
                    AttributeValue::S(ref_type.to_string()),
                ));
            }
        }
        attributes
    }

    pub fn condition_expressions(&self) -> Vec<String> {
        vec!["attribute_not_exists(#pk)".to_string()]
    }

    pub fn expressions_attribute_names(&self) -> HashMap<String, String> {
        HashMap::from([("#pk".to_string(), "pk".to_string())])
    }

    fn common(&self) -> &TombstoneCommon {
        match self {
            Tombstone::Scope { common }
            | Tombstone::Repository { common, .. }
            | Tombstone::Ref { common, .. } => common,
        }
    }
}

impl TombstoneCommon {
    fn attributes(&self) -> Vec<(String, AttributeValue)> {
        let mut attributes = vec![
            ("pk".to_string(), AttributeValue::S(self.pk.clone())),
            ("sk".to_string(), AttributeValue::S(self.sk.clone())),
            (
                "provider".to_string(),
                AttributeValue::S(self.provider.to_string()),
            ),
            ("host".to_string(), AttributeValue::S(self.host.clone())),
            (
                "reason".to_string(),
                AttributeValue::S(self.reason.to_string()),
            ),
            (
                "deleted_at_millis".to_string(),
                AttributeValue::N(self.deleted_at_millis.to_string()),
            ),
            (
                "last_seen_at_millis".to_string(),
                AttributeValue::N(self.last_seen_at_millis.to_string()),
            ),
            (
                "detected_via".to_string(),
                AttributeValue::S(self.detected_via.to_string()),
            ),
            (
                "delivery_count".to_string(),
                AttributeValue::N(self.delivery_count.to_string()),
            ),
            (
                "audit_pk".to_string(),
                AttributeValue::S(self.audit_pk.clone()),
            ),
            (
                "audit_sk".to_string(),
                AttributeValue::S(self.audit_sk.clone()),
            ),
        ];

        if let Some(scope_type) = self.scope_type {
            attributes.push((
                "scope_type".to_string(),
                AttributeValue::S(scope_type.to_string()),
            ));
        }
        if let Some(scope_id) = &self.scope_id {
            attributes.push(("scope_id".to_string(), AttributeValue::S(scope_id.clone())));
        }
        if let Some(scope_pk) = &self.scope_pk {
            attributes.push(("scope_pk".to_string(), AttributeValue::S(scope_pk.clone())));
        }
        if let Some(scope_sk) = &self.scope_sk {
            attributes.push(("scope_sk".to_string(), AttributeValue::S(scope_sk.clone())));
        }

        attributes
    }

    fn build(
        pk: String,
        sk: String,
        ctx: &TombstoneContext,
        entity_type: &str,
        natural_id: &str,
        enclosing: Option<EnclosingScope>,
    ) -> Self {
        let hour = ctx.deleted_at_millis / 3_600_000;
        let audit_pk = format!("DEL#{}#{}#{}", ctx.host, entity_type, hour);
        let audit_sk = format!("{}#{}", ctx.deleted_at_millis, natural_id);

        let (scope_type, scope_id, scope_pk, scope_sk) = match enclosing {
            Some(s) => (
                Some(s.scope_type),
                Some(s.scope_id.clone()),
                Some(format!("HOST#{}#SCOPE#{}", ctx.host, s.scope_id)),
                Some(format!("DEL#{}#{}", entity_type, natural_id)),
            ),
            None => (None, None, None, None),
        };

        Self {
            pk,
            sk,
            provider: ctx.provider,
            host: ctx.host.clone(),
            reason: ctx.reason,
            deleted_at_millis: ctx.deleted_at_millis,
            last_seen_at_millis: ctx.deleted_at_millis,
            detected_via: ctx.detected_via,
            delivery_count: 1,
            scope_type,
            scope_id,
            scope_pk,
            scope_sk,
            audit_pk,
            audit_sk,
        }
    }
}

impl TombstoneContext {
    pub fn new(source: &GitSource, reason: Reason, detected_via: DetectedVia) -> Self {
        let provider = match source {
            GitSource::GitHub { .. } => Provider::GitHub,
        };
        let host = match source {
            GitSource::GitHub { host, .. } => host.clone(),
        };
        let scope = match source {
            GitSource::GitHub {
                installation_id, ..
            } => Some(EnclosingScope {
                scope_id: installation_id.to_string(),
                scope_type: ScopeType::Installation,
            }),
        };
        Self {
            provider,
            host,
            reason,
            detected_via,
            deleted_at_millis: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            scope,
        }
    }
}
