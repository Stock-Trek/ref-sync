use crate::{
    aws::Aws,
    dto::event_converter::{EventConverter, EventConverters},
    error::{RsWebhookError, RsWebhookResult},
    infisical::secrets::Secrets,
};
use aws_config::SdkConfig;
use ref_sync_shared::{deadline::Deadline, env_vars::EnvVars, timeouts::Timeouts};
use std::{
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};
use tokio::sync::{Mutex, RwLock};

const COLD_START_REFRESH_TIMEOUT_KEY: &str = "WEBHOOK_COLD_START_REFRESH_TIMEOUT";
const REFRESH_COOLDOWN_KEY: &str = "WEBHOOK_REFRESH_COOLDOWN";
const DEFAULT_COLD_START_REFRESH_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_REFRESH_COOLDOWN: Duration = Duration::from_secs(30);

pub type Converters = Arc<Vec<Box<dyn EventConverter>>>;

pub struct RsWebhookContext {
    pub aws: Aws,
    sdk_config: SdkConfig,
    timeouts: Timeouts,
    converters: RwLock<Option<Converters>>,
    refresh: Mutex<Option<Instant>>,
    refresh_cooldown: Duration,
}

impl RsWebhookContext {
    pub async fn new() -> RsWebhookResult<Self> {
        let aws_timeouts = Aws::load_timeouts()?;
        let sdk_config = Aws::load_sdk_config(aws_timeouts).await;
        let aws = Aws::new(&sdk_config, aws_timeouts)?;
        let refresh_cooldown = EnvVars::duration(REFRESH_COOLDOWN_KEY, DEFAULT_REFRESH_COOLDOWN)?;
        let context = Self {
            aws,
            sdk_config,
            timeouts: aws_timeouts,
            converters: RwLock::new(None),
            refresh: Mutex::new(None),
            refresh_cooldown,
        };
        // Warm the cache eagerly, but tolerate a transient Infisical outage:
        // startup must not fail, and the first request will retry.
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

    /// Returns the current converters, loading secrets on first use.
    pub async fn converters(&self, deadline: Deadline) -> RsWebhookResult<Converters> {
        if let Some(converters) = self.converters.read().await.clone() {
            return Ok(converters);
        }
        let mut last_refresh = self.refresh.lock().await;
        if let Some(converters) = self.converters.read().await.clone() {
            return Ok(converters);
        }
        self.load(&mut last_refresh, deadline).await
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
        let mut last_refresh = self.refresh.lock().await;
        match self.load(&mut last_refresh, deadline).await {
            Ok(converters) => Ok(Some(converters)),
            Err(RsWebhookError::RefreshThrottled) => Ok(None),
            Err(error) => {
                *last_refresh = Some(Instant::now());
                Err(error)
            }
        }
    }

    async fn load(
        &self,
        last_refresh: &mut Option<Instant>,
        deadline: Deadline,
    ) -> RsWebhookResult<Converters> {
        if last_refresh.is_some_and(|instant| instant.elapsed() < self.refresh_cooldown) {
            return Err(RsWebhookError::RefreshThrottled);
        }
        let secrets = Secrets::load(&self.sdk_config, self.timeouts, deadline).await?;
        let converters = Arc::new(EventConverters::all(&secrets)?);
        *self.converters.write().await = Some(converters.clone());
        *last_refresh = Some(Instant::now());
        Ok(converters)
    }
}
