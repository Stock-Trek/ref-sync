use crate::{
    aws::Aws,
    dto::event_converter::{EventConverter, EventConverters},
    error::{RsWebhookError, RsWebhookResult},
    infisical::secrets::Secrets,
};
use aws_config::SdkConfig;
use ref_sync_shared::{deadline::Deadline, timeouts::Timeouts};
use std::{
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};
use tokio::sync::{Mutex, RwLock};

const REFRESH_COOLDOWN: Duration = Duration::from_secs(30);
/// Budget for the best-effort cold-start secret load. Lambda bounds the init
/// phase separately, but the credential provider chain is otherwise unbounded.
const COLD_START_REFRESH_TIMEOUT: Duration = Duration::from_secs(30);

pub type Converters = Arc<Vec<Box<dyn EventConverter>>>;

pub struct RsWebhookContext {
    pub aws: Aws,
    sdk_config: SdkConfig,
    timeouts: Timeouts,
    converters: RwLock<Option<Converters>>,
    refresh: Mutex<Option<Instant>>,
}

impl RsWebhookContext {
    pub async fn new() -> RsWebhookResult<Self> {
        let aws_timeouts = Aws::load_timeouts()?;
        let sdk_config = Aws::load_sdk_config(aws_timeouts).await;
        let aws = Aws::new(&sdk_config, aws_timeouts)?;
        let context = Self {
            aws,
            sdk_config,
            timeouts: aws_timeouts,
            converters: RwLock::new(None),
            refresh: Mutex::new(None),
        };
        // Warm the cache eagerly, but tolerate a transient Infisical outage:
        // startup must not fail, and the first request will retry.
        let deadline = Deadline::new(SystemTime::now() + COLD_START_REFRESH_TIMEOUT);
        if let Err(error) = context.refresh_converters(deadline).await {
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
            Err(error) => Err(error),
        }
    }

    async fn load(
        &self,
        last_refresh: &mut Option<Instant>,
        deadline: Deadline,
    ) -> RsWebhookResult<Converters> {
        if last_refresh.is_some_and(|instant| instant.elapsed() < REFRESH_COOLDOWN) {
            return Err(RsWebhookError::RefreshThrottled);
        }
        *last_refresh = Some(Instant::now());
        let secrets = Secrets::load(&self.sdk_config, self.timeouts, deadline).await?;
        let converters = Arc::new(EventConverters::all(&secrets)?);
        *self.converters.write().await = Some(converters.clone());
        Ok(converters)
    }
}
