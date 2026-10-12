use crate::tombstones::enums::{DetectedVia, EnclosingScope, Provider, Reason, ScopeType};
use ref_sync_shared::sqs_message::GitSource;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct TombstoneContext {
    pub provider: Provider,
    pub host: String,
    pub reason: Reason,
    pub detected_via: DetectedVia,
    pub deleted_at_millis: u64,
    pub scope: Option<EnclosingScope>,
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
