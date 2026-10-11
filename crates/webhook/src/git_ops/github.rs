use crate::{
    error::{RsWebhookResult, RsWebhookError},
    git_ops::git_host::GitHost,
};
use async_trait::async_trait;
use jsonwebtoken::EncodingKey;
use octocrab::{Octocrab, models::InstallationId};
use ref_sync_shared::{env_vars::EnvVars, timeouts::Timeouts};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};

const GITHUB_PER_PAGE: u32 = 100;
const GITHUB_PROVIDER: &str = "github";

const DEFAULT_GITHUB_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const DEFAULT_GITHUB_OPERATION_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_CACHED_INSTALLATIONS: usize = 128;

const GITHUB_APP_ID: &str = "GITHUB_APP_ID";
const GITHUB_APP_PRIVATE_KEY: &str = "GITHUB_APP_PRIVATE_KEY";

pub struct GitHubAppConfig {
    pub app_id: u64,
    pub private_key: String,
}

impl GitHubAppConfig {
    pub fn from_env() -> RsWebhookResult<Self> {
        let app_id = EnvVars::env_var(GITHUB_APP_ID)?;
        let app_id = app_id.parse().map_err(|error| {
            RsWebhookError::Config(format!("{GITHUB_APP_ID} is invalid: {error}"))
        })?;
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

/// A least-recently-used cache of installation clients.
///
/// The number of GitHub App installations is unbounded, so the cache must be
/// bounded to avoid growing without limit over the lifetime of the process.
struct InstallationCache {
    capacity: usize,
    entries: HashMap<u64, GitHub>,
    order: VecDeque<u64>,
}

impl InstallationCache {
    fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    fn get(&mut self, installation_id: u64) -> Option<GitHub> {
        let github = self.entries.get(&installation_id)?.clone();
        self.touch(installation_id);
        Some(github)
    }

    fn insert(&mut self, installation_id: u64, github: GitHub) {
        if let std::collections::hash_map::Entry::Occupied(mut entry) =
            self.entries.entry(installation_id)
        {
            entry.insert(github);
            self.touch(installation_id);
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
        self.entries.insert(installation_id, github);
        self.order.push_back(installation_id);
    }

    fn touch(&mut self, installation_id: u64) {
        if let Some(index) = self.order.iter().position(|id| *id == installation_id) {
            self.order.remove(index);
        }
        self.order.push_back(installation_id);
    }
}

#[derive(Clone)]
pub struct GitHubAuth {
    app: Octocrab,
    installations: Arc<Mutex<InstallationCache>>,
    timeouts: Timeouts,
}

impl GitHubAuth {
    pub fn new() -> RsWebhookResult<Self> {
        let config = GitHubAppConfig::from_env()?;
        let timeouts = Timeouts::from_env(
            "GITHUB",
            DEFAULT_GITHUB_CONNECT_TIMEOUT,
            DEFAULT_GITHUB_OPERATION_TIMEOUT,
        )?;
        let key = EncodingKey::from_rsa_pem(config.private_key.as_bytes()).map_err(|error| {
            RsWebhookError::Config(format!("GitHub App private key is invalid: {error}"))
        })?;
        let app = Octocrab::builder()
            .app(config.app_id.into(), key)
            .set_connect_timeout(Some(timeouts.connect))
            .set_read_timeout(Some(timeouts.operation))
            .build()
            .map_err(|error| {
                RsWebhookError::GitHub(format!("failed to build GitHub client: {error}"))
            })?;
        Ok(Self {
            app,
            installations: Arc::new(Mutex::new(InstallationCache::new(MAX_CACHED_INSTALLATIONS))),
            timeouts,
        })
    }

    pub fn installation(&self, installation_id: u64) -> RsWebhookResult<GitHub> {
        let mut installations = self.installations.lock().map_err(|_| {
            RsWebhookError::InternalServer("GitHub installation cache lock poisoned".into())
        })?;
        if let Some(github) = installations.get(installation_id) {
            return Ok(github);
        }
        let github = GitHub::new(&self.app, installation_id, self.timeouts)?;
        installations.insert(installation_id, github.clone());
        Ok(github)
    }
}

#[derive(Clone)]
pub struct GitHub {
    client: Octocrab,
    installation_id: u64,
    timeouts: Timeouts,
}

#[async_trait]
impl GitHost for GitHub {
    fn provider(&self) -> &'static str {
        GITHUB_PROVIDER
    }

    fn installation_id(&self) -> String {
        self.installation_id.to_string()
    }

    async fn repo_ids(&self, deadline: SystemTime) -> RsWebhookResult<Vec<String>> {
        let repos = self
            .timeouts
            .with_deadline(deadline, "GitHub list repositories", self.repos())
            .await?;
        Ok(repos?.into_iter().map(|repo| repo.id.to_string()).collect())
    }

    async fn repo_default_branch(&self, repo_id: &str, deadline: SystemTime) -> RsWebhookResult<String> {
        let repo_id: u64 = repo_id.parse().map_err(|_| {
            RsWebhookError::InternalServer("Failed to parse GitHub repository id".into())
        })?;
        let repo = self
            .timeouts
            .with_deadline(deadline, "GitHub get repository", self.repo(repo_id))
            .await?;
        repo.map(|r| r.default_branch)
    }
}

// helpers
impl GitHub {
    fn new(app: &Octocrab, installation_id: u64, timeouts: Timeouts) -> RsWebhookResult<Self> {
        let client = app
            .installation(InstallationId::from(installation_id))
            .map_err(|error| {
                RsWebhookError::GitHub(format!(
                    "failed to create client for installation {installation_id}: {error}"
                ))
            })?;
        Ok(Self {
            client,
            installation_id,
            timeouts,
        })
    }

    async fn repos(&self) -> RsWebhookResult<Vec<GitHubRepo>> {
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
                    RsWebhookError::GitHub(format!(
                        "failed to list installation repositories: {error}"
                    ))
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

    async fn repo(&self, repo_id: u64) -> RsWebhookResult<GitHubRepo> {
        self.client
            .get::<GitHubRepo, _, _>(format!("/repositories/{repo_id}"), None::<&()>)
            .await
            .map_err(|error| {
                RsWebhookError::GitHub(format!("failed to get repository {repo_id}: {error}"))
            })
    }
}
