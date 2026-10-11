use crate::{
    error::{RsWebhookError, RsWebhookResult},
    infisical::auth_flow::InfisicalAwsAuth,
};
use aws_config::SdkConfig;
use ref_sync_shared::env_vars::EnvVars;
use serde::Deserialize;
use std::{collections::HashMap, time::Duration};

const HTTP_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const HTTP_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

const INFISICAL_IDENTITY_ID: &str = "INFISICAL_IDENTITY_ID";
const INFISICAL_PROJECT_ID: &str = "INFISICAL_PROJECT_ID";
const INFISICAL_ENVIRONMENT: &str = "INFISICAL_ENVIRONMENT";
const INFISICAL_BASE_URL: &str = "INFISICAL_BASE_URL";

pub struct Secrets(HashMap<String, String>);

impl Secrets {
    pub async fn load(sdk_config: &SdkConfig) -> RsWebhookResult<Self> {
        let config = InfisicalConfig::from_env()?;
        let http_client = reqwest::ClientBuilder::default()
            .connect_timeout(HTTP_CONNECT_TIMEOUT)
            .timeout(HTTP_REQUEST_TIMEOUT)
            .build()
            .map_err(RsWebhookError::Reqwest)?;
        let auth = InfisicalAwsAuth::new(http_client, config.base_url);
        let secrets = auth
            .try_load_secrets(
                sdk_config,
                &config.identity_id,
                &config.project_id,
                &config.environment,
            )
            .await?;
        Ok(Self(secrets))
    }

    pub(crate) fn get(&self, name: &str) -> RsWebhookResult<String> {
        self.0
            .get(name)
            .cloned()
            .ok_or_else(|| RsWebhookError::Config(format!("missing required config: {name}")))
    }
}

struct InfisicalConfig {
    identity_id: String,
    project_id: String,
    environment: String,
    base_url: String,
}

impl InfisicalConfig {
    fn from_env() -> RsWebhookResult<Self> {
        Ok(Self {
            identity_id: EnvVars::env_var(INFISICAL_IDENTITY_ID)?,
            project_id: EnvVars::env_var(INFISICAL_PROJECT_ID)?,
            environment: EnvVars::env_var(INFISICAL_ENVIRONMENT)?,
            base_url: EnvVars::env_var(INFISICAL_BASE_URL)?,
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
