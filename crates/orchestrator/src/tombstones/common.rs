use crate::tombstones::{
    context::TombstoneContext,
    enums::{TombstoneDetectedVia, EnclosingScope, TombstoneProvider, TombstoneReason, TombstoneScopeType},
};
use aws_sdk_dynamodb::types::AttributeValue;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const SCOPE_SK: &str = "DEL#SCOPE";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TombstoneCommon {
    pk: String,
    sk: String,

    provider: TombstoneProvider,
    host: String,
    reason: TombstoneReason,
    deleted_at_millis: u64,
    last_seen_at_millis: u64,
    detected_via: TombstoneDetectedVia,
    delivery_count: u32,

    #[serde(skip_serializing_if = "Option::is_none")]
    scope_type: Option<TombstoneScopeType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scope_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scope_pk: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scope_sk: Option<String>,
    audit_pk: String,
    audit_sk: String,
}

impl TombstoneCommon {
    pub fn new(
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
                Some(SCOPE_SK.to_string()),
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
    pub fn pk(&self) -> &str {
        &self.pk
    }
    pub fn sk(&self) -> &str {
        &self.sk
    }
    pub fn attributes(&self) -> HashMap<String, AttributeValue> {
        let mut attributes = HashMap::new();
        Self::add_attribute_string(&mut attributes, "pk", &self.pk);
        Self::add_attribute_string(&mut attributes, "sk", &self.sk);
        Self::add_attribute_string(&mut attributes, "provider", &self.provider);
        Self::add_attribute_string(&mut attributes, "host", &self.host);
        Self::add_attribute_string(&mut attributes, "reason", &self.reason);
        Self::add_attribute_u64(&mut attributes, "deleted_at_millis", self.deleted_at_millis);
        Self::add_attribute_u64(
            &mut attributes,
            "last_seen_at_millis",
            self.last_seen_at_millis,
        );
        Self::add_attribute_string(&mut attributes, "detected_via", self.detected_via);
        Self::add_attribute_u32(&mut attributes, "delivery_count", self.delivery_count);
        Self::add_attribute_string(&mut attributes, "audit_pk", &self.audit_pk);
        Self::add_attribute_string(&mut attributes, "audit_sk", &self.audit_sk);
        Self::add_attribute_opt_string(&mut attributes, "scope_type", &self.scope_type);
        Self::add_attribute_opt_string(&mut attributes, "scope_id", &self.scope_id);
        Self::add_attribute_opt_string(&mut attributes, "scope_pk", &self.scope_pk);
        Self::add_attribute_opt_string(&mut attributes, "scope_sk", &self.scope_sk);
        attributes
    }
    fn add_attribute_u32(attributes: &mut HashMap<String, AttributeValue>, name: &str, value: u32) {
        attributes.insert(name.into(), AttributeValue::N(value.to_string()));
    }
    fn add_attribute_u64(attributes: &mut HashMap<String, AttributeValue>, name: &str, value: u64) {
        attributes.insert(name.into(), AttributeValue::N(value.to_string()));
    }
    fn add_attribute_string(
        attributes: &mut HashMap<String, AttributeValue>,
        name: &str,
        value: impl ToString,
    ) {
        attributes.insert(name.into(), AttributeValue::S(value.to_string()));
    }
    fn add_attribute_opt_string(
        attributes: &mut HashMap<String, AttributeValue>,
        name: &str,
        value: &Option<impl ToString>,
    ) {
        if let Some(value) = value {
            attributes.insert(name.into(), AttributeValue::S(value.to_string()));
        }
    }
}
