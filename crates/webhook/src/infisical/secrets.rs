use crate::{
    error::{RsWebhookError, RsWebhookResult},
    infisical::auth_flow::InfisicalAwsAuth,
};
use aws_config::SdkConfig;
use ref_sync_shared::{
    deadline::Deadline,
    env_vars::EnvVars,
    errors::{config::RsConfigError, error::RsSharedError},
    timeouts::Timeouts,
};
use serde::Deserialize;
use std::collections::HashMap;

const INFISICAL_IDENTITY_ID: &str = "INFISICAL_IDENTITY_ID";
const INFISICAL_PROJECT_ID: &str = "INFISICAL_PROJECT_ID";
const INFISICAL_ENVIRONMENT: &str = "INFISICAL_ENVIRONMENT";
const INFISICAL_BASE_URL: &str = "INFISICAL_BASE_URL";

#[derive(Clone)]
pub struct Secrets(HashMap<String, String>);

impl Secrets {
    pub async fn load(
        sdk_config: &SdkConfig,
        timeouts: Timeouts,
        deadline: Deadline,
    ) -> RsWebhookResult<Self> {
        let config = InfisicalConfig::from_env()?;
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
        if let Some(connect_timeout) = timeouts.connect {
            builder = builder.connect_timeout(connect_timeout);
        }
        if let Some(request_timeout) = timeouts.operation {
            builder = builder.timeout(request_timeout);
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
    fn from_env() -> RsWebhookResult<Self> {
        Ok(Self {
            identity_id: EnvVars::env_var(INFISICAL_IDENTITY_ID)?,
            project_id: EnvVars::env_var(INFISICAL_PROJECT_ID)?,
            environment: EnvVars::env_var(INFISICAL_ENVIRONMENT)?,
            base_url: validated_base_url(&EnvVars::env_var(INFISICAL_BASE_URL)?)?,
        })
    }
}

fn validated_base_url(base_url: &str) -> RsWebhookResult<String> {
    let url = reqwest::Url::parse(base_url).map_err(|error| {
        RsWebhookError::Shared(RsSharedError::Config(RsConfigError::invalid_source(
            INFISICAL_BASE_URL,
            error,
        )))
    })?;
    if url.scheme() != "https" {
        return Err(RsWebhookError::Shared(RsSharedError::Config(
            RsConfigError::invalid(
                INFISICAL_BASE_URL,
                format!("must use the \"https\" scheme, got \"{}\"", url.scheme()),
            ),
        )));
    }
    Ok(base_url.to_owned())
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
