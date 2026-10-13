use crate::tombstones::{
    common::TombstoneCommon,
    context::TombstoneContext,
    enums::{EnclosingScope, RefType, TombstoneScopeType},
};
use aws_sdk_dynamodb::types::AttributeValue;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const SCOPE_SK: &str = "DEL#SCOPE";

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

impl Tombstone {
    pub fn scope(ctx: &TombstoneContext, scope_type: TombstoneScopeType, scope_id: String) -> Self {
        let pk = TombstoneCommon::enclosing_scope_pk(&ctx.host, &scope_id);
        let sk = SCOPE_SK.to_string();
        let common = TombstoneCommon::new(
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
        let pk = Self::scoped_pk(ctx, "REPO", &repo_id);
        let sk = "DEL#REPOSITORY".to_string();
        let common = TombstoneCommon::new(pk, sk, ctx, "REPOSITORY", &repo_id, ctx.scope.clone());
        Tombstone::Repository { common, repo_id }
    }

    pub fn reference(
        ctx: &TombstoneContext,
        repo_id: String,
        ref_name: String,
        ref_type: RefType,
    ) -> Self {
        let pk = Self::scoped_pk(ctx, "REPO", &repo_id);
        let sk = format!("DEL#REF#{}#{}", ref_type, ref_name);
        let natural_id = format!("{}#{}#{}", repo_id, ref_type, ref_name);
        let common = TombstoneCommon::new(pk, sk, ctx, "REF", &natural_id, ctx.scope.clone());
        Tombstone::Ref {
            common,
            repo_id,
            ref_name,
            ref_type,
        }
    }
    pub fn attributes(&self) -> HashMap<String, AttributeValue> {
        let mut attributes = self.common().attributes();
        match self {
            Tombstone::Scope { .. } => {}
            Tombstone::Repository { repo_id, .. } => {
                attributes.insert("repo_id".to_string(), AttributeValue::S(repo_id.clone()));
            }
            Tombstone::Ref {
                repo_id,
                ref_name,
                ref_type,
                ..
            } => {
                attributes.insert("repo_id".to_string(), AttributeValue::S(repo_id.clone()));
                attributes.insert("ref_name".to_string(), AttributeValue::S(ref_name.clone()));
                attributes.insert(
                    "ref_type".to_string(),
                    AttributeValue::S(ref_type.to_string()),
                );
            }
        }
        attributes
    }

    pub fn keys(&self) -> HashMap<String, AttributeValue> {
        let common = self.common();
        HashMap::from([
            ("pk".to_string(), AttributeValue::S(common.pk().into())),
            ("sk".to_string(), AttributeValue::S(common.sk().into())),
        ])
    }

    fn scoped_pk(ctx: &TombstoneContext, entity: &str, natural_id: &str) -> String {
        match &ctx.scope {
            Some(scope) => format!(
                "{}#{}#{}",
                TombstoneCommon::enclosing_scope_pk(&ctx.host, &scope.scope_id),
                entity,
                natural_id
            ),
            None => format!("HOST#{}#{}#{}", ctx.host, entity, natural_id),
        }
    }

    fn common(&self) -> &TombstoneCommon {
        match self {
            Tombstone::Scope { common }
            | Tombstone::Repository { common, .. }
            | Tombstone::Ref { common, .. } => common,
        }
    }
}
