use crate::{
    error::{ConfigError, GitHubError, InternalServerError, RsSharedError, RsSharedResult},
    git_ops::github::{
        app_config::GitHubAppConfig,
        host_allow_list::{DEFAULT_HOST, HostAllowlist},
        installation::GitHubInstallation,
    },
    timeouts::Timeouts,
};
use jsonwebtoken::EncodingKey;
use lru::LruCache;
use octocrab::Octocrab;
use std::{
    collections::HashMap,
    num::NonZeroUsize,
    sync::{Arc, Mutex},
    time::Duration,
};

const ENTERPRISE_API_PATH: &str = "api/v3";
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const DEFAULT_OPERATION_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_CACHED_INSTALLATIONS: usize = 128;

type InstallationKey = (String, u64);

#[derive(Clone)]
pub struct GitHub {
    app_id: u64,
    key: EncodingKey,
    apps: Arc<Mutex<HashMap<String, Octocrab>>>,
    installations: Arc<Mutex<LruCache<InstallationKey, GitHubInstallation>>>,
    allowed_hosts: HostAllowlist,
    timeouts: Timeouts,
}

impl GitHub {
    pub fn new() -> RsSharedResult<Self> {
        let config = GitHubAppConfig::from_env()?;
        let timeouts =
            Timeouts::from_env("GITHUB", DEFAULT_CONNECT_TIMEOUT, DEFAULT_OPERATION_TIMEOUT)?;
        let key = EncodingKey::from_rsa_pem(config.private_key.as_bytes())
            .map_err(|error| ConfigError::invalid_source("GitHub App private key", error))?;
        Ok(Self {
            app_id: config.app_id,
            key,
            apps: Arc::new(Mutex::new(HashMap::new())),
            installations: Arc::new(Mutex::new(LruCache::new(
                NonZeroUsize::new(MAX_CACHED_INSTALLATIONS).unwrap(),
            ))),
            allowed_hosts: HostAllowlist::from_env()?,
            timeouts,
        })
    }

    pub fn installation(
        &self,
        host: &str,
        installation_id: u64,
    ) -> RsSharedResult<GitHubInstallation> {
        if !self.allowed_hosts.allows(host) {
            return Err(RsSharedError::GitHub(GitHubError::HostNotAllowed {
                host: host.to_owned(),
            }));
        }
        let key = (host.to_owned(), installation_id);
        let mut installations = self.installations.lock().map_err(|_| {
            RsSharedError::InternalServer(InternalServerError::LockPoisoned {
                resource: "GitHub installation cache",
            })
        })?;
        if let Some(installation) = installations.get(&key) {
            return Ok(installation.clone());
        }
        let app = self.app(host)?;
        let github = GitHubInstallation::new(&app, host, installation_id, self.timeouts)?;
        installations.put(key, github.clone());
        Ok(github)
    }

    /// Returns an authenticated GitHub App client for `host`, building and
    /// caching it on first use. The public GitHub host uses octocrab's default
    /// base URI; any other host is treated as GitHub Enterprise and routed
    /// through `https://<host>/api/v3`.
    fn app(&self, host: &str) -> RsSharedResult<Octocrab> {
        let mut apps = self.apps.lock().map_err(|_| {
            RsSharedError::InternalServer(InternalServerError::LockPoisoned {
                resource: "GitHub app cache",
            })
        })?;
        if let Some(app) = apps.get(host) {
            return Ok(app.clone());
        }
        let mut builder = Octocrab::builder().app(self.app_id.into(), self.key.clone());
        if host != DEFAULT_HOST {
            let base_uri = format!("https://{host}/{ENTERPRISE_API_PATH}");
            builder = builder.base_uri(base_uri).map_err(|error| {
                ConfigError::invalid_source(format!("GitHub Enterprise host {host}"), error)
            })?;
        }
        let app = builder
            .set_connect_timeout(Some(self.timeouts.connect))
            .set_read_timeout(Some(self.timeouts.operation))
            .build()
            .map_err(|error| RsSharedError::GitHub(GitHubError::BuildClient(error)))?;
        apps.insert(host.to_owned(), app.clone());
        Ok(app)
    }
}
