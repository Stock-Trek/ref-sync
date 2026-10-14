use crate::{
    aws::Aws,
    dto::event_converter::{EventConverter, EventConverters},
    error::{RsWebhookError, RsWebhookResult},
    infisical::secrets::Secrets,
    metrics::{
        record_secret_cache_miss, record_secret_load_attempt, record_secret_load_backoff_hit,
        record_secret_load_failure, record_secret_load_in_flight_wait, record_secret_load_success,
    },
};
use aws_config::SdkConfig;
use ref_sync_shared::{deadline::Deadline, env_vars::EnvVars, timeouts::Timeouts};
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime},
};
use tokio::sync::{Mutex, MutexGuard, RwLock};

const COLD_START_REFRESH_TIMEOUT_KEY: &str = "WEBHOOK_COLD_START_REFRESH_TIMEOUT";
const REFRESH_COOLDOWN_KEY: &str = "WEBHOOK_REFRESH_COOLDOWN";
const FAILURE_BACKOFF_KEY: &str = "WEBHOOK_SECRET_FAILURE_BACKOFF";
const DEFAULT_COLD_START_REFRESH_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_REFRESH_COOLDOWN: Duration = Duration::from_secs(30);
const DEFAULT_FAILURE_BACKOFF: Duration = Duration::from_secs(5);
const MAX_JITTER_RATIO: f64 = 0.5;
static JITTER_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub type Converters = Arc<Vec<Box<dyn EventConverter>>>;

#[derive(Clone, Copy, PartialEq, Eq)]
enum LoadReason {
    CacheMiss,
    SignatureRefresh,
}

pub struct RsWebhookContext {
    pub aws: Aws,
    sdk_config: SdkConfig,
    timeouts: Timeouts,
    converters: RwLock<Option<Converters>>,
    load: Mutex<()>,
    refreshed_at: Mutex<Option<Instant>>,
    backoff_until: Mutex<Option<Instant>>,
    refresh_cooldown: Duration,
    failure_backoff: Duration,
}

impl RsWebhookContext {
    pub async fn new() -> RsWebhookResult<Self> {
        let aws_timeouts = Aws::load_timeouts()?;
        let sdk_config = Aws::load_sdk_config(aws_timeouts).await;
        let aws = Aws::from_env(&sdk_config, aws_timeouts)?;
        let refresh_cooldown = EnvVars::duration(REFRESH_COOLDOWN_KEY, DEFAULT_REFRESH_COOLDOWN)?;
        let failure_backoff = EnvVars::duration(FAILURE_BACKOFF_KEY, DEFAULT_FAILURE_BACKOFF)?;
        let context = Self {
            aws,
            sdk_config,
            timeouts: aws_timeouts,
            converters: RwLock::new(None),
            load: Mutex::new(()),
            refreshed_at: Mutex::new(None),
            backoff_until: Mutex::new(None),
            refresh_cooldown,
            failure_backoff,
        };
        // Warm the cache eagerly, but tolerate a transient Infisical outage:
        // startup must not fail, and the first request will retry once the
        // failure backoff expires.
        let cold_start_refresh_timeout = EnvVars::duration(
            COLD_START_REFRESH_TIMEOUT_KEY,
            DEFAULT_COLD_START_REFRESH_TIMEOUT,
        )?;
        let deadline = Deadline::new(SystemTime::now() + cold_start_refresh_timeout);
        if let Err(error) = context.converters(deadline).await {
            tracing::warn!("failed to load secrets during initialisation: {error}");
        }
        Ok(context)
    }

    /// Returns the current converters, loading secrets on first use. Failed
    /// loads are throttled by a jittered backoff so a flood of unauthenticated
    /// webhooks cannot amplify into unbounded Infisical and STS traffic.
    pub async fn converters(&self, deadline: Deadline) -> RsWebhookResult<Converters> {
        if let Some(converters) = self.converters.read().await.clone() {
            return Ok(converters);
        }
        record_secret_cache_miss();
        self.load_converters(deadline, LoadReason::CacheMiss).await
    }

    /// Reloads secrets from Infisical and rebuilds the converters, returning
    /// `None` when a refresh happened too recently. Serialising refreshes and
    /// rate limiting them stops forged webhooks from amplifying into a stream
    /// of outbound auth calls; callers should fall back to the cached
    /// converters (and reject the request) when `None` is returned.
    pub async fn refresh_converters(
        &self,
        deadline: Deadline,
    ) -> RsWebhookResult<Option<Converters>> {
        match self
            .load_converters(deadline, LoadReason::SignatureRefresh)
            .await
        {
            Ok(converters) => Ok(Some(converters)),
            Err(RsWebhookError::RefreshThrottled) => Ok(None),
            Err(RsWebhookError::SecretsUnavailable { .. }) => Ok(None),
            Err(error) => Err(error),
        }
    }

    async fn load_converters(
        &self,
        deadline: Deadline,
        reason: LoadReason,
    ) -> RsWebhookResult<Converters> {
        let _guard = self.acquire_load_lock().await;

        if reason == LoadReason::CacheMiss {
            if let Some(converters) = self.converters.read().await.clone() {
                return Ok(converters);
            }
        } else if self.refreshed_recently().await {
            return Err(RsWebhookError::RefreshThrottled);
        }

        if let Some(retry_after) = self.remaining_backoff().await {
            record_secret_load_backoff_hit();
            return Err(RsWebhookError::SecretsUnavailable { retry_after });
        }

        record_secret_load_attempt();
        match self.try_load(deadline).await {
            Ok(converters) => {
                *self.converters.write().await = Some(converters.clone());
                *self.refreshed_at.lock().await = Some(Instant::now());
                *self.backoff_until.lock().await = None;
                record_secret_load_success();
                Ok(converters)
            }
            Err(error) => {
                record_secret_load_failure(error.error_type());
                tracing::warn!("failed to load secrets: {error}");
                let retry_after = self.jittered_backoff();
                *self.backoff_until.lock().await = Some(Instant::now() + retry_after);
                Err(RsWebhookError::SecretsUnavailable { retry_after })
            }
        }
    }

    async fn try_load(&self, deadline: Deadline) -> RsWebhookResult<Converters> {
        let secrets = Secrets::load(&self.sdk_config, self.timeouts, deadline).await?;
        Ok(Arc::new(EventConverters::all(&secrets)?))
    }

    async fn acquire_load_lock(&self) -> MutexGuard<'_, ()> {
        match self.load.try_lock() {
            Ok(guard) => guard,
            Err(_) => {
                record_secret_load_in_flight_wait();
                self.load.lock().await
            }
        }
    }

    async fn refreshed_recently(&self) -> bool {
        self.refreshed_at
            .lock()
            .await
            .is_some_and(|instant| instant.elapsed() < self.refresh_cooldown)
    }

    async fn remaining_backoff(&self) -> Option<Duration> {
        let until = (*self.backoff_until.lock().await)?;
        let now = Instant::now();
        (now < until).then(|| until - now)
    }

    fn jittered_backoff(&self) -> Duration {
        let sequence = JITTER_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let entropy = sequence.wrapping_mul(6364136223846793005).wrapping_add(
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_nanos() as u64),
        );
        let fraction = (entropy >> 40) as f64 / f64::from(1u32 << 24);
        self.failure_backoff
            .mul_f64(1.0 + fraction * MAX_JITTER_RATIO)
    }
}
