use crate::{
    env_vars::EnvVars,
    error::{
        ConfigError, GitHubError, InternalServerError, InvalidMessageError, RefSyncSharedError,
        SharedResult, TimeoutError,
    },
    git_ops::git_host::GitHost,
    timeouts::{Deadline, Timeouts},
};
use async_trait::async_trait;
use jsonwebtoken::EncodingKey;
use octocrab::{Octocrab, models::InstallationId};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{Arc, Mutex},
    time::Duration,
};

const GITHUB_PER_PAGE: u32 = 100;
const GITHUB_PROVIDER: &str = "github";

/// The host of the public GitHub API. GitHub Enterprise hosts are treated as
/// non-default and routed through `https://<host>/api/v3`.
pub const DEFAULT_GITHUB_HOST: &str = "github.com";
const GITHUB_ENTERPRISE_API_PATH: &str = "api/v3";

const DEFAULT_GITHUB_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const DEFAULT_GITHUB_OPERATION_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_CACHED_INSTALLATIONS: usize = 128;

const GITHUB_APP_ID: &str = "GITHUB_APP_ID";
const GITHUB_APP_PRIVATE_KEY: &str = "GITHUB_APP_PRIVATE_KEY";
/// Additional, non-default GitHub hosts that outbound GitHub App credentials
/// may be sent to. The public host ([`DEFAULT_GITHUB_HOST`]) is always allowed
/// and does not need to be listed here.
const GITHUB_ENTERPRISE_ALLOWED_HOSTS: &str = "GITHUB_ENTERPRISE_ALLOWED_HOSTS";

/// Hosts that outbound GitHub App credentials may be sent to.
///
/// The host on an inbound webhook is taken from the payload, so it is
/// attacker-controlled. It must be checked against this allowlist before an
/// app JWT is built for it to prevent credential egress on a leaked webhook
/// secret.
///
/// The public GitHub host ([`DEFAULT_GITHUB_HOST`]) is always allowed. Any
/// other (GitHub Enterprise) hosts must be supplied via
/// `GITHUB_ENTERPRISE_ALLOWED_HOSTS`.
#[derive(Debug, Clone)]
struct HostAllowlist {
    hosts: HashSet<String>,
}

impl HostAllowlist {
    fn from_env() -> SharedResult<Self> {
        let mut hosts = HashSet::from([DEFAULT_GITHUB_HOST.to_owned()]);
        match std::env::var(GITHUB_ENTERPRISE_ALLOWED_HOSTS) {
            Ok(value) => hosts.extend(Self::parse(&value)?),
            Err(std::env::VarError::NotPresent) => {}
            Err(std::env::VarError::NotUnicode(_)) => {
                return Err(RefSyncSharedError::Config(ConfigError::not_unicode(
                    GITHUB_ENTERPRISE_ALLOWED_HOSTS,
                )));
            }
        }
        Ok(Self { hosts })
    }

    fn parse(value: &str) -> SharedResult<HashSet<String>> {
        let hosts = value
            .split(',')
            .map(str::trim)
            .filter(|host| !host.is_empty())
            .map(str::to_ascii_lowercase)
            .collect::<HashSet<_>>();
        if hosts.is_empty() {
            return Err(RefSyncSharedError::Config(ConfigError::empty(
                GITHUB_ENTERPRISE_ALLOWED_HOSTS,
            )));
        }
        Ok(hosts)
    }

    fn allows(&self, host: &str) -> bool {
        self.hosts.contains(&host.to_ascii_lowercase())
    }
}

pub struct GitHubAppConfig {
    pub app_id: u64,
    pub private_key: String,
}

impl GitHubAppConfig {
    pub fn from_env() -> SharedResult<Self> {
        let app_id = EnvVars::env_var(GITHUB_APP_ID)?;
        let app_id = app_id
            .parse()
            .map_err(|error| ConfigError::invalid_source(GITHUB_APP_ID, error))?;
        Ok(Self {
            app_id,
            private_key: EnvVars::env_var(GITHUB_APP_PRIVATE_KEY)?.replace("\\n", "\n"),
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
struct GitHubRepo {
    id: u64,
    default_branch: String,
}

#[derive(Debug, Deserialize)]
struct InstallationRepositoriesResponse {
    repositories: Vec<GitHubRepo>,
}

#[derive(Debug, Serialize)]
struct RepositoriesParams {
    per_page: u32,
    page: u32,
}

/// Key for an installation client. The host is part of the key because the
/// same installation id can exist on different GitHub hosts.
type InstallationKey = (String, u64);

/// A least-recently-used cache of installation clients.
///
/// The number of installations is unbounded across hosts, so the cache must be
/// bounded to avoid growing without limit over the lifetime of the process. The
/// host is part of the key because the same installation id can exist on
/// different GitHub hosts.
struct InstallationCache {
    capacity: usize,
    entries: HashMap<InstallationKey, GitHub>,
    order: VecDeque<InstallationKey>,
}

impl InstallationCache {
    fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    fn get(&mut self, key: &InstallationKey) -> Option<GitHub> {
        let github = self.entries.get(key)?.clone();
        self.touch(key);
        Some(github)
    }

    fn insert(&mut self, key: InstallationKey, github: GitHub) {
        if let std::collections::hash_map::Entry::Occupied(mut entry) =
            self.entries.entry(key.clone())
        {
            entry.insert(github);
            self.touch(&key);
            return;
        }
        while self.entries.len() >= self.capacity {
            match self.order.pop_front() {
                Some(evicted) => {
                    self.entries.remove(&evicted);
                }
                None => break,
            }
        }
        self.entries.insert(key.clone(), github);
        self.order.push_back(key);
    }

    fn touch(&mut self, key: &InstallationKey) {
        if let Some(index) = self.order.iter().position(|cached| cached == key) {
            self.order.remove(index);
        }
        self.order.push_back(key.clone());
    }
}

#[derive(Clone)]
pub struct GitHubAuth {
    app_id: u64,
    key: EncodingKey,
    apps: Arc<Mutex<HashMap<String, Octocrab>>>,
    installations: Arc<Mutex<InstallationCache>>,
    allowed_hosts: HostAllowlist,
    timeouts: Timeouts,
}

impl GitHubAuth {
    pub fn new() -> SharedResult<Self> {
        let config = GitHubAppConfig::from_env()?;
        let timeouts = Timeouts::from_env(
            "GITHUB",
            DEFAULT_GITHUB_CONNECT_TIMEOUT,
            DEFAULT_GITHUB_OPERATION_TIMEOUT,
        )?;
        let key = EncodingKey::from_rsa_pem(config.private_key.as_bytes())
            .map_err(|error| ConfigError::invalid_source("GitHub App private key", error))?;
        Ok(Self {
            app_id: config.app_id,
            key,
            apps: Arc::new(Mutex::new(HashMap::new())),
            installations: Arc::new(Mutex::new(InstallationCache::new(MAX_CACHED_INSTALLATIONS))),
            allowed_hosts: HostAllowlist::from_env()?,
            timeouts,
        })
    }

    pub fn installation(&self, host: &str, installation_id: u64) -> SharedResult<GitHub> {
        if !self.allowed_hosts.allows(host) {
            return Err(RefSyncSharedError::GitHub(GitHubError::HostNotAllowed {
                host: host.to_owned(),
            }));
        }
        let key = (host.to_owned(), installation_id);
        let mut installations = self.installations.lock().map_err(|_| {
            RefSyncSharedError::InternalServer(InternalServerError::LockPoisoned {
                resource: "GitHub installation cache",
            })
        })?;
        if let Some(github) = installations.get(&key) {
            return Ok(github);
        }
        let app = self.app(host)?;
        let github = GitHub::new(&app, host, installation_id, self.timeouts)?;
        installations.insert(key, github.clone());
        Ok(github)
    }

    /// Returns an authenticated GitHub App client for `host`, building and
    /// caching it on first use. The public GitHub host uses octocrab's default
    /// base URI; any other host is treated as GitHub Enterprise and routed
    /// through `https://<host>/api/v3`.
    fn app(&self, host: &str) -> SharedResult<Octocrab> {
        let mut apps = self.apps.lock().map_err(|_| {
            RefSyncSharedError::InternalServer(InternalServerError::LockPoisoned {
                resource: "GitHub app cache",
            })
        })?;
        if let Some(app) = apps.get(host) {
            return Ok(app.clone());
        }
        let mut builder = Octocrab::builder().app(self.app_id.into(), self.key.clone());
        if host != DEFAULT_GITHUB_HOST {
            let base_uri = format!("https://{host}/{GITHUB_ENTERPRISE_API_PATH}");
            builder = builder.base_uri(base_uri).map_err(|error| {
                ConfigError::invalid_source(format!("GitHub Enterprise host {host}"), error)
            })?;
        }
        let app = builder
            .set_connect_timeout(Some(self.timeouts.connect))
            .set_read_timeout(Some(self.timeouts.operation))
            .build()
            .map_err(|error| RefSyncSharedError::GitHub(GitHubError::BuildClient(error)))?;
        apps.insert(host.to_owned(), app.clone());
        Ok(app)
    }
}

#[derive(Clone)]
pub struct GitHub {
    client: Octocrab,
    host: String,
    installation_id: u64,
    timeouts: Timeouts,
}

#[async_trait]
impl GitHost for GitHub {
    fn provider(&self) -> &'static str {
        GITHUB_PROVIDER
    }

    fn host(&self) -> &str {
        &self.host
    }

    fn installation_id(&self) -> String {
        self.installation_id.to_string()
    }

    async fn repo_ids(&self, deadline: Deadline) -> SharedResult<Vec<String>> {
        let timeout = deadline.operation_timeout(&self.timeouts)?;
        let repos = tokio::time::timeout(timeout, self.repos())
            .await
            .map_err(|_| {
                RefSyncSharedError::Timeout(TimeoutError::Operation {
                    operation: "GitHub list repositories",
                })
            })??;
        Ok(repos.into_iter().map(|repo| repo.id.to_string()).collect())
    }

    async fn repo_default_branch(&self, repo_id: &str, deadline: Deadline) -> SharedResult<String> {
        let repo_id: u64 = repo_id.parse().map_err(|error| {
            RefSyncSharedError::InvalidMessage(InvalidMessageError::GitHubRepositoryId {
                repo_id: repo_id.to_owned(),
                source: error,
            })
        })?;
        let timeout = deadline.operation_timeout(&self.timeouts)?;
        let repo = tokio::time::timeout(timeout, self.repo(repo_id))
            .await
            .map_err(|_| {
                RefSyncSharedError::Timeout(TimeoutError::Operation {
                    operation: "GitHub get repository",
                })
            })?;
        repo.map(|r| r.default_branch)
    }
}

// helpers
impl GitHub {
    fn new(
        app: &Octocrab,
        host: &str,
        installation_id: u64,
        timeouts: Timeouts,
    ) -> SharedResult<Self> {
        let client = app
            .installation(InstallationId::from(installation_id))
            .map_err(|error| {
                RefSyncSharedError::GitHub(GitHubError::Installation {
                    installation_id,
                    source: error,
                })
            })?;
        Ok(Self {
            client,
            host: host.to_owned(),
            installation_id,
            timeouts,
        })
    }

    async fn repos(&self) -> SharedResult<Vec<GitHubRepo>> {
        let mut repos = Vec::new();
        let mut page = 1;
        loop {
            let params = RepositoriesParams {
                per_page: GITHUB_PER_PAGE,
                page,
            };
            let response = self
                .client
                .get::<InstallationRepositoriesResponse, _, _>(
                    "/installation/repositories",
                    Some(&params),
                )
                .await
                .map_err(|error| {
                    RefSyncSharedError::GitHub(GitHubError::ListRepositories(error))
                })?;
            let received = response.repositories.len();
            repos.extend(response.repositories);
            if received < GITHUB_PER_PAGE as usize {
                break;
            }
            page += 1;
        }
        Ok(repos)
    }

    async fn repo(&self, repo_id: u64) -> SharedResult<GitHubRepo> {
        self.client
            .get::<GitHubRepo, _, _>(format!("/repositories/{repo_id}"), None::<&()>)
            .await
            .map_err(|error| {
                if is_not_found(&error) {
                    RefSyncSharedError::GitHub(GitHubError::RepositoryNotFound {
                        repo_id,
                        source: error,
                    })
                } else {
                    RefSyncSharedError::GitHub(GitHubError::GetRepository {
                        repo_id,
                        source: error,
                    })
                }
            })
    }
}

fn is_not_found(error: &octocrab::Error) -> bool {
    matches!(
        error,
        octocrab::Error::GitHub { source, .. } if source.status_code.as_u16() == 404
    )
}
