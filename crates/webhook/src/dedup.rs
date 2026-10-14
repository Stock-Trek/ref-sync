use crate::error::RsWebhookResult;
use lru::LruCache;
use parking_lot::Mutex;
use ref_sync_shared::{
    env_vars::EnvVars,
    errors::{config::RsConfigError, error::RsSharedError},
};
use std::{
    num::NonZeroUsize,
    sync::Arc,
    time::{Duration, Instant},
};

const MAX_DELIVERIES_KEY: &str = "WEBHOOK_DEDUP_MAX_DELIVERIES";
const TTL_KEY: &str = "WEBHOOK_DEDUP_TTL";
const DEFAULT_MAX_DELIVERIES: usize = 4096;
const DEFAULT_TTL: Duration = Duration::from_secs(3600);

/// Best-effort, in-process record of recently seen GitHub delivery IDs.
///
/// Lambda reuses a warm execution environment between invocations, so a replayed
/// `x-github-delivery` is dropped here before it is forwarded to SQS. The cache
/// is bounded and entries expire, so a long-lived environment cannot grow
/// without limit. Redeliveries handled by another environment are still deduped
/// downstream by the ECS client token.
#[derive(Clone)]
pub struct DeliveryDedup {
    seen: Arc<Mutex<LruCache<String, Instant>>>,
    ttl: Duration,
}

impl DeliveryDedup {
    pub fn from_env() -> RsWebhookResult<Self> {
        let capacity =
            NonZeroUsize::new(EnvVars::usize(MAX_DELIVERIES_KEY, DEFAULT_MAX_DELIVERIES)?)
                .ok_or_else(|| {
                    RsSharedError::Config(RsConfigError::invalid(
                        MAX_DELIVERIES_KEY,
                        "must be greater than zero",
                    ))
                })?;
        let ttl = EnvVars::duration(TTL_KEY, DEFAULT_TTL)?;
        Ok(Self::new(capacity, ttl))
    }

    pub fn new(capacity: NonZeroUsize, ttl: Duration) -> Self {
        Self {
            seen: Arc::new(Mutex::new(LruCache::new(capacity))),
            ttl,
        }
    }

    /// Marks `delivery_id` as seen and reports whether it was already seen
    /// within the TTL window, in which case the delivery is a duplicate.
    pub fn is_duplicate(&self, delivery_id: &str) -> bool {
        let mut seen = self.seen.lock();
        if seen
            .get(delivery_id)
            .is_some_and(|inserted| inserted.elapsed() < self.ttl)
        {
            return true;
        }
        seen.put(delivery_id.to_owned(), Instant::now());
        false
    }

    /// Removes `delivery_id` so a later retry can be processed after a failed
    /// attempt.
    pub fn forget(&self, delivery_id: &str) {
        self.seen.lock().pop(delivery_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capacity(value: usize) -> NonZeroUsize {
        NonZeroUsize::new(value).expect("capacity must be positive")
    }

    #[test]
    fn reports_duplicate_within_ttl() {
        let dedup = DeliveryDedup::new(capacity(4), Duration::from_secs(3600));
        assert!(!dedup.is_duplicate("delivery-1"));
        assert!(dedup.is_duplicate("delivery-1"));
    }

    #[test]
    fn tracks_different_deliveries_independently() {
        let dedup = DeliveryDedup::new(capacity(4), Duration::from_secs(3600));
        assert!(!dedup.is_duplicate("delivery-1"));
        assert!(!dedup.is_duplicate("delivery-2"));
        assert!(dedup.is_duplicate("delivery-1"));
        assert!(dedup.is_duplicate("delivery-2"));
    }

    #[test]
    fn forget_allows_reprocessing() {
        let dedup = DeliveryDedup::new(capacity(4), Duration::from_secs(3600));
        assert!(!dedup.is_duplicate("delivery-1"));
        dedup.forget("delivery-1");
        assert!(!dedup.is_duplicate("delivery-1"));
    }

    #[test]
    fn expired_entries_are_not_duplicates() {
        let dedup = DeliveryDedup::new(capacity(4), Duration::ZERO);
        assert!(!dedup.is_duplicate("delivery-1"));
        assert!(!dedup.is_duplicate("delivery-1"));
    }

    #[test]
    fn evicts_least_recently_used_delivery() {
        let dedup = DeliveryDedup::new(capacity(1), Duration::from_secs(3600));
        assert!(!dedup.is_duplicate("delivery-1"));
        assert!(!dedup.is_duplicate("delivery-2"));
        assert!(!dedup.is_duplicate("delivery-1"));
    }
}
