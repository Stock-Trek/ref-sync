use crate::{
    deadline::Deadline,
    errors::{
        error::{RsSharedError, RsSharedResult},
        github::RsGitHubError,
        invalid_message::RsInvalidMessageError,
        timeout::RsTimeoutError,
    },
    git_ops::git_remote::{GitFile, GitHost},
    timeouts::Timeouts,
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use octocrab::{Octocrab, models::InstallationId};
use serde::{Deserialize, Serialize};
use std::{path::Path, sync::Arc};

const PROVIDER: &str = "github";
const REPOS_PER_PAGE: u32 = 100;

#[derive(Debug, Serialize)]
struct RepositoriesParams {
    per_page: u32,
    page: u32,
}

#[derive(Debug, Serialize)]
struct CommitsParams<'a> {
    sha: &'a str,
    per_page: u32,
}

#[derive(Debug, Serialize)]
struct ContentsParams<'a> {
    #[serde(rename = "ref")]
    git_ref: &'a str,
}

#[derive(Debug, Deserialize)]
struct GitHubCommit {
    sha: String,
}

#[derive(Debug, Deserialize)]
struct GitHubContent {
    #[serde(rename = "type")]
    kind: String,
    path: String,
    sha: String,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    encoding: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GitHubBlob {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    encoding: Option<String>,
}

#[derive(Debug, Deserialize)]
struct InstallationRepositoriesResponse {
    repositories: Vec<GitHubRepo>,
}

#[derive(Debug, Clone, Deserialize)]
struct GitHubRepo {
    id: u64,
    default_branch: String,
}

#[derive(Clone)]
pub struct GitHubInstallation {
    client: Arc<Octocrab>,
    host: String,
    installation_id: u64,
    timeouts: Timeouts,
}

impl GitHost<u64, u64> for GitHubInstallation {
    fn provider(&self) -> &'static str {
        PROVIDER
    }

    fn host(&self) -> &str {
        &self.host
    }

    fn installation_id(&self) -> u64 {
        self.installation_id
    }

    fn parse_repo_id(&self, repo_id: &str) -> RsSharedResult<u64> {
        repo_id.parse().map_err(|error| {
            RsSharedError::InvalidMessage(RsInvalidMessageError::GitHubRepositoryId {
                repo_id: repo_id.to_owned(),
                source: error,
            })
        })
    }

    async fn repo_ids(&self, deadline: Deadline) -> RsSharedResult<Vec<u64>> {
        let timeout = self.timeouts.operation_timeout(deadline)?;
        let repos = tokio::time::timeout(timeout, self.repos())
            .await
            .map_err(|_| {
                RsSharedError::Timeout(RsTimeoutError::Operation {
                    operation: "GitHub list repositories",
                })
            })??;
        Ok(repos.into_iter().map(|repo| repo.id).collect())
    }

    async fn repo_default_branch(
        &self,
        repo_id: u64,
        deadline: Deadline,
    ) -> RsSharedResult<String> {
        let timeout = self.timeouts.operation_timeout(deadline)?;
        let repo = tokio::time::timeout(timeout, self.repo(repo_id))
            .await
            .map_err(|_| {
                RsSharedError::Timeout(RsTimeoutError::Operation {
                    operation: "GitHub get repository",
                })
            })?;
        repo.map(|r| r.default_branch)
    }

    async fn ref_commit(
        &self,
        repo_id: u64,
        ref_name: &str,
        deadline: Deadline,
    ) -> RsSharedResult<String> {
        let timeout = self.timeouts.operation_timeout(deadline)?;
        let params = CommitsParams {
            sha: ref_name,
            per_page: 1,
        };
        let commits = tokio::time::timeout(
            timeout,
            self.client.get::<Vec<GitHubCommit>, _, _>(
                format!("/repositories/{repo_id}/commits"),
                Some(&params),
            ),
        )
        .await
        .map_err(|_| {
            RsSharedError::Timeout(RsTimeoutError::Operation {
                operation: "GitHub get commit",
            })
        })?
        .map_err(|error| {
            RsSharedError::GitHub(RsGitHubError::GetCommit {
                repo_id,
                ref_name: ref_name.to_owned(),
                source: error,
            })
        })?;
        commits
            .into_iter()
            .next()
            .map(|commit| commit.sha)
            .ok_or_else(|| {
                RsSharedError::GitHub(RsGitHubError::CommitNotFound {
                    repo_id,
                    ref_name: ref_name.to_owned(),
                })
            })
    }

    async fn directory_files(
        &self,
        repo_id: u64,
        ref_name: &str,
        directory: &str,
        deadline: Deadline,
    ) -> RsSharedResult<Vec<GitFile>> {
        let timeout = self.timeouts.operation_timeout(deadline)?;
        tokio::time::timeout(
            timeout,
            self.collect_directory_files(repo_id, ref_name, directory),
        )
        .await
        .map_err(|_| {
            RsSharedError::Timeout(RsTimeoutError::Operation {
                operation: "GitHub get repository contents",
            })
        })?
    }
}

impl GitHubInstallation {
    pub fn new(
        app: &Octocrab,
        host: &str,
        installation_id: u64,
        timeouts: Timeouts,
    ) -> RsSharedResult<Self> {
        let client = app
            .installation(InstallationId::from(installation_id))
            .map_err(|error| {
                RsSharedError::GitHub(RsGitHubError::Installation {
                    installation_id,
                    source: error,
                })
            })?;
        Ok(Self {
            client: Arc::new(client),
            host: host.to_owned(),
            installation_id,
            timeouts,
        })
    }
}

// helpers
impl GitHubInstallation {
    async fn collect_directory_files(
        &self,
        repo_id: u64,
        ref_name: &str,
        directory: &str,
    ) -> RsSharedResult<Vec<GitFile>> {
        let directory = directory.trim_matches('/');
        let prefix = format!("{directory}/");
        let mut files = Vec::new();
        let mut pending = vec![directory.to_owned()];
        while let Some(path) = pending.pop() {
            for entry in self.contents(repo_id, &path, ref_name).await? {
                match entry.kind.as_str() {
                    "dir" => pending.push(entry.path),
                    "file" => {
                        let relative = entry.path.strip_prefix(&prefix).ok_or_else(|| {
                            RsSharedError::GitHub(RsGitHubError::InvalidContentPath {
                                repo_id,
                                path: entry.path.clone(),
                                directory: directory.to_owned(),
                            })
                        })?;
                        Self::validate_relative_path(repo_id, relative, directory)?;
                        let contents = match Self::decode_content(
                            repo_id,
                            &entry.path,
                            entry.content.as_deref(),
                            entry.encoding.as_deref(),
                        )? {
                            Some(contents) => contents,
                            None => self.blob(repo_id, &entry.sha).await?,
                        };
                        files.push(GitFile {
                            path: relative.to_owned(),
                            contents,
                        });
                    }
                    "symlink" | "submodule" => {}
                    kind => {
                        return Err(RsSharedError::GitHub(
                            RsGitHubError::UnsupportedContentType {
                                repo_id,
                                path: entry.path,
                                content_type: kind.to_owned(),
                            },
                        ));
                    }
                }
            }
        }
        files.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(files)
    }

    async fn contents(
        &self,
        repo_id: u64,
        path: &str,
        ref_name: &str,
    ) -> RsSharedResult<Vec<GitHubContent>> {
        let params = ContentsParams { git_ref: ref_name };
        self.client
            .get::<Vec<GitHubContent>, _, _>(
                format!(
                    "/repositories/{repo_id}/contents/{}",
                    Self::encode_path(path)
                ),
                Some(&params),
            )
            .await
            .map_err(|error| {
                if Self::is_not_found(&error) {
                    RsSharedError::GitHub(RsGitHubError::ContentNotFound {
                        repo_id,
                        path: path.to_owned(),
                        ref_name: ref_name.to_owned(),
                        source: Box::new(error),
                    })
                } else {
                    RsSharedError::GitHub(RsGitHubError::GetContent {
                        repo_id,
                        path: path.to_owned(),
                        source: Box::new(error),
                    })
                }
            })
    }

    async fn blob(&self, repo_id: u64, sha: &str) -> RsSharedResult<Vec<u8>> {
        let blob = self
            .client
            .get::<GitHubBlob, _, _>(
                format!("/repositories/{repo_id}/git/blobs/{sha}"),
                None::<&()>,
            )
            .await
            .map_err(|source| {
                RsSharedError::GitHub(RsGitHubError::GetBlob {
                    repo_id,
                    sha: sha.to_owned(),
                    source: Box::new(source),
                })
            })?;
        Self::decode_content(
            repo_id,
            sha,
            blob.content.as_deref(),
            blob.encoding.as_deref(),
        )?
        .ok_or_else(|| {
            RsSharedError::GitHub(RsGitHubError::MissingContent {
                repo_id,
                path: sha.to_owned(),
            })
        })
    }

    fn decode_content(
        repo_id: u64,
        path: &str,
        content: Option<&str>,
        encoding: Option<&str>,
    ) -> RsSharedResult<Option<Vec<u8>>> {
        let Some(content) = content else {
            return Ok(None);
        };
        match encoding {
            Some("base64") => {
                let compact = content
                    .chars()
                    .filter(|character| !character.is_ascii_whitespace())
                    .collect::<String>();
                BASE64.decode(compact).map(Some).map_err(|source| {
                    RsSharedError::GitHub(RsGitHubError::DecodeContent {
                        repo_id,
                        path: path.to_owned(),
                        encoding: "base64",
                        source,
                    })
                })
            }
            Some("utf8") => Ok(Some(content.as_bytes().to_vec())),
            Some("none") | None if content.is_empty() => Ok(None),
            Some(encoding) => Err(RsSharedError::GitHub(
                RsGitHubError::UnsupportedContentEncoding {
                    repo_id,
                    path: path.to_owned(),
                    encoding: encoding.to_owned(),
                },
            )),
            None => Ok(Some(content.as_bytes().to_vec())),
        }
    }

    fn encode_path(path: &str) -> String {
        path.split('/')
            .map(|segment| {
                let mut encoded = String::new();
                for byte in segment.bytes() {
                    match byte {
                        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                            encoded.push(char::from(byte))
                        }
                        byte => encoded.push_str(&format!("%{byte:02X}")),
                    }
                }
                encoded
            })
            .collect::<Vec<_>>()
            .join("/")
    }

    fn validate_relative_path(repo_id: u64, path: &str, directory: &str) -> RsSharedResult<()> {
        let path = Path::new(path);
        if path.as_os_str().is_empty()
            || path.to_string_lossy().contains('\\')
            || path
                .components()
                .any(|component| !matches!(component, std::path::Component::Normal(_)))
        {
            return Err(RsSharedError::GitHub(RsGitHubError::InvalidContentPath {
                repo_id,
                path: path.display().to_string(),
                directory: directory.to_owned(),
            }));
        }
        Ok(())
    }

    async fn repos(&self) -> RsSharedResult<Vec<GitHubRepo>> {
        let mut repos = Vec::new();
        let mut page = 1;
        loop {
            let params = RepositoriesParams {
                per_page: REPOS_PER_PAGE,
                page,
            };
            let response = self
                .client
                .get::<InstallationRepositoriesResponse, _, _>(
                    "/installation/repositories",
                    Some(&params),
                )
                .await
                .map_err(|error| RsSharedError::GitHub(RsGitHubError::ListRepositories(error)))?;
            let received = response.repositories.len();
            repos.extend(response.repositories);
            if received < REPOS_PER_PAGE as usize {
                break;
            }
            page += 1;
        }
        Ok(repos)
    }

    async fn repo(&self, repo_id: u64) -> RsSharedResult<GitHubRepo> {
        self.client
            .get::<GitHubRepo, _, _>(format!("/repositories/{repo_id}"), None::<&()>)
            .await
            .map_err(|error| {
                if Self::is_not_found(&error) {
                    RsSharedError::GitHub(RsGitHubError::RepositoryNotFound {
                        repo_id,
                        source: error,
                    })
                } else {
                    RsSharedError::GitHub(RsGitHubError::GetRepository {
                        repo_id,
                        source: error,
                    })
                }
            })
    }

    fn is_not_found(error: &octocrab::Error) -> bool {
        matches!(
            error,
            octocrab::Error::GitHub { source, .. } if source.status_code.as_u16() == 404
        )
    }
}
