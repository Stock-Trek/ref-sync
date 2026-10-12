use crate::{
    env_vars::EnvVars,
    errors::{config::RsConfigError, error::RsSharedResult},
};

const GITHUB_APP_ID: &str = "GITHUB_APP_ID";
const GITHUB_APP_PRIVATE_KEY: &str = "GITHUB_APP_PRIVATE_KEY";

pub struct GitHubAppConfig {
    pub app_id: u64,
    pub private_key: String,
}

impl GitHubAppConfig {
    pub fn from_env() -> RsSharedResult<Self> {
        let app_id = EnvVars::env_var(GITHUB_APP_ID)?;
        let app_id = app_id
            .parse()
            .map_err(|error| RsConfigError::invalid_source(GITHUB_APP_ID, error))?;
        Ok(Self {
            app_id,
            private_key: EnvVars::env_var(GITHUB_APP_PRIVATE_KEY)?.replace("\\n", "\n"),
        })
    }
}
