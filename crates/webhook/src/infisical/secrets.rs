use crate::{
    error::{RsWebhookError, RsWebhookResult},
    infisical::auth_flow::InfisicalAwsAuth,
};
use aws_config::SdkConfig;
use ref_sync_shared::{
    deadline::Deadline,
    env_vars::EnvVarConfig,
    errors::{config::RsConfigError, error::RsSharedError},
    timeouts::{Timeout, Timeouts},
};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Clone)]
pub struct Secrets(HashMap<String, String>);

impl Secrets {
    pub async fn load(
        sdk_config: &SdkConfig,
        timeouts: Timeouts,
        deadline: Deadline,
        config: &EnvVarConfig,
    ) -> RsWebhookResult<Self> {
        let config = InfisicalConfig::from_env(config)?;
        let http_client = Self::http_client(timeouts)?;
        let auth = InfisicalAwsAuth::new(http_client, config.base_url, timeouts);
        let secrets = auth
            .try_load_secrets(
                sdk_config,
                deadline,
                &config.identity_id,
                &config.project_id,
                &config.environment,
            )
            .await?;
        Ok(Self(secrets))
    }

    fn http_client(timeouts: Timeouts) -> RsWebhookResult<reqwest::Client> {
        let mut builder = reqwest::ClientBuilder::default();
        if let Timeout::Bounded(connect_timeout) = timeouts.connect {
            builder = builder.connect_timeout(connect_timeout.get());
        }
        if let Timeout::Bounded(request_timeout) = timeouts.operation {
            builder = builder.timeout(request_timeout.get());
        }
        builder.build().map_err(RsWebhookError::Reqwest)
    }

    pub(crate) fn get(&self, name: &str) -> RsWebhookResult<String> {
        self.0.get(name).cloned().ok_or_else(|| {
            RsWebhookError::Shared(RsSharedError::Config(RsConfigError::missing(name)))
        })
    }
}

struct InfisicalConfig {
    identity_id: String,
    project_id: String,
    environment: String,
    base_url: String,
}

impl InfisicalConfig {
    fn from_env(config: &EnvVarConfig) -> RsWebhookResult<Self> {
        Ok(Self {
            identity_id: config.infisical_identity_id()?,
            project_id: config.infisical_project_id()?,
            environment: config.infisical_environment()?,
            base_url: config.infisical_base_url()?,
        })
    }
}

#[derive(Deserialize)]
pub struct InfisicalSecretsResponse {
    pub secrets: Vec<InfisicalSecretResponse>,
}

#[derive(Deserialize)]
pub struct InfisicalSecretResponse {
    #[serde(rename = "secretKey")]
    pub secret_key: String,
    #[serde(rename = "secretValue")]
    pub secret_value: String,
}
