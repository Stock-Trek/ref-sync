use crate::{
    aws::Aws,
    dto::event_converter::{EventConverter, EventConverters},
    error::RsWebhookResult,
    infisical::secrets::Secrets,
};
use aws_config::SdkConfig;
use std::sync::Arc;
use tokio::sync::RwLock;

pub type Converters = Arc<Vec<Box<dyn EventConverter>>>;

pub struct RsWebhookContext {
    pub aws: Aws,
    sdk_config: SdkConfig,
    converters: RwLock<Option<Converters>>,
}

impl RsWebhookContext {
    pub async fn new() -> RsWebhookResult<Self> {
        let aws_timeouts = Aws::load_timeouts()?;
        let sdk_config = Aws::load_sdk_config(aws_timeouts).await;
        let aws = Aws::new(&sdk_config, aws_timeouts)?;
        let context = Self {
            aws,
            sdk_config,
            converters: RwLock::new(None),
        };
        // Warm the cache eagerly, but tolerate a transient Infisical outage:
        // startup must not fail, and the first request will retry.
        if let Err(error) = context.refresh_converters().await {
            tracing::warn!("failed to load secrets during initialisation: {error}");
        }
        Ok(context)
    }

    /// Returns the current converters, loading secrets on first use.
    pub async fn converters(&self) -> RsWebhookResult<Converters> {
        let cached = self.converters.read().await.clone();
        if let Some(converters) = cached {
            return Ok(converters);
        }
        self.refresh_converters().await
    }

    /// Reloads secrets from Infisical and rebuilds the converters. The result
    /// is only cached when every required secret is present, so a partial or
    /// failed load is retried on the next request.
    pub async fn refresh_converters(&self) -> RsWebhookResult<Converters> {
        let secrets = Secrets::load(&self.sdk_config).await?;
        let converters = Arc::new(EventConverters::all(&secrets)?);
        *self.converters.write().await = Some(converters.clone());
        Ok(converters)
    }
}
