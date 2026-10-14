use crate::{env_vars::EnvVarConfig, errors::error::RsSharedResult};

pub struct GitHubAppConfig {
    pub app_id: u64,
    pub private_key: String,
}

impl GitHubAppConfig {
    pub fn from_env(config: &EnvVarConfig) -> RsSharedResult<Self> {
        Ok(Self {
            app_id: config.github_app_id()?,
            private_key: config.github_app_private_key()?,
        })
    }
}
