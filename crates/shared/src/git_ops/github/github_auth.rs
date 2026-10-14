use crate::{
    env_vars::EnvVarConfig,
    errors::{
        config::RsConfigError,
        error::{RsSharedError, RsSharedResult},
        github::RsGitHubError,
    },
    git_ops::github::{
        app_config::GitHubAppConfig,
        host_allow_list::{DEFAULT_HOST, HostAllowlist, RsHostError},
        installation::GitHubInstallation,
    },
    timeouts::Timeouts,
};
use jsonwebtoken::EncodingKey;
use lru::LruCache;
use octocrab::Octocrab;
use parking_lot::Mutex;
use std::{collections::HashMap, num::NonZeroUsize, sync::Arc, time::Duration};

const ENTERPRISE_API_PATH: &str = "api/v3";
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const DEFAULT_OPERATION_TIMEOUT: Duration = Duration::from_secs(120);

type InstallationKey = (String, u64);

#[derive(Clone)]
pub struct GitHubAuth {
    app_id: u64,
    key: EncodingKey,
    apps: Arc<Mutex<HashMap<String, Arc<Octocrab>>>>,
    installations: Arc<Mutex<LruCache<InstallationKey, GitHubInstallation>>>,
    allowed_hosts: HostAllowlist,
    timeouts: Timeouts,
}

impl GitHubAuth {
    pub fn new(
        config: GitHubAppConfig,
        timeouts: Timeouts,
        allowed_hosts: HostAllowlist,
        max_cached_installations: NonZeroUsize,
    ) -> RsSharedResult<Self> {
        let key = EncodingKey::from_rsa_pem(config.private_key.as_bytes())
            .map_err(|error| RsConfigError::invalid_source("GitHub App private key", error))?;
        Ok(Self {
            app_id: config.app_id,
            key,
            apps: Arc::new(Mutex::new(HashMap::new())),
            installations: Arc::new(Mutex::new(LruCache::new(max_cached_installations))),
            allowed_hosts,
            timeouts,
        })
    }

    pub fn from_env(config: &EnvVarConfig) -> RsSharedResult<Self> {
        let app = GitHubAppConfig::from_env(config)?;
        let timeouts = Timeouts::from_env(
            config,
            "GITHUB",
            DEFAULT_CONNECT_TIMEOUT,
            DEFAULT_OPERATION_TIMEOUT,
        )?;
        let allowed_hosts = HostAllowlist::from_env(config)?;
        let max_cached_installations = config.github_max_cached_installations()?;
        Self::new(app, timeouts, allowed_hosts, max_cached_installations)
    }

    pub fn installation(
        &self,
        host: &str,
        installation_id: u64,
    ) -> RsSharedResult<GitHubInstallation> {
        let host = self
            .allowed_hosts
            .validate(host)
            .map_err(|error| match error {
                RsHostError::Invalid { host } => {
                    RsSharedError::GitHub(RsGitHubError::InvalidHost { host })
                }
                RsHostError::NotAllowed { host } => {
                    RsSharedError::GitHub(RsGitHubError::HostNotAllowed { host })
                }
            })?;
        let key = (host.clone(), installation_id);
        let mut installations = self.installations.lock();
        if let Some(installation) = installations.get(&key) {
            return Ok(installation.clone());
        }
        let app = self.app(&host)?;
        let github = GitHubInstallation::new(app.as_ref(), &host, installation_id, self.timeouts)?;
        installations.put(key, github.clone());
        Ok(github)
    }

    /// Returns an authenticated GitHub App client for `host`, building and
    /// caching it on first use. The public GitHub host uses octocrab's default
    /// base URI; any other host is treated as GitHub Enterprise and routed
    /// through `https://<host>/api/v3`.
    fn app(&self, host: &str) -> RsSharedResult<Arc<Octocrab>> {
        let mut apps = self.apps.lock();
        if let Some(app) = apps.get(host) {
            return Ok(Arc::clone(app));
        }
        let mut builder = Octocrab::builder().app(self.app_id.into(), self.key.clone());
        if host != DEFAULT_HOST {
            let base_uri = format!("https://{host}/{ENTERPRISE_API_PATH}");
            builder = builder.base_uri(base_uri).map_err(|error| {
                RsConfigError::invalid_source(format!("GitHub Enterprise host {host}"), error)
            })?;
        }
        let app = Arc::new(
            builder
                .set_connect_timeout(self.timeouts.connect.into())
                .set_read_timeout(self.timeouts.operation.into())
                .build()
                .map_err(|error| RsSharedError::GitHub(RsGitHubError::BuildClient(error)))?,
        );
        apps.insert(host.to_owned(), Arc::clone(&app));
        Ok(app)
    }
}
