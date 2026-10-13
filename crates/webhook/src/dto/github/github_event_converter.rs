use crate::{
    dto::{
        event_converter::EventConverter,
        github::{
            github_event_installation::{GitHubEventInstallation, GitHubEventInstallationAction},
            github_event_installation_repositories::{
                GitHubEventInstallationRepositories, GitHubEventInstallationRepositoriesAction,
            },
            github_event_push::GitHubEventPush,
            github_event_ref::GitHubEventRef,
            github_event_repository::{GitHubEventRepository, GitHubEventRepositoryAction},
            github_event_type::GitHubEventType,
            github_types::{
                GitHubInstallation, GitHubRefType, GitHubRepository, GitHubRepositorySelection,
            },
        },
    },
    error::{RsWebhookError, RsWebhookResult},
    event_response::EventResponse,
    infisical::secrets::Secrets,
};
use hmac::{Hmac, KeyInit, Mac};
use ref_sync_shared::{
    errors::{config::RsConfigError, error::RsSharedError},
    sqs_message::{
        GitSource, SqsDetail, SqsMessage, SqsMultiRepoAction, SqsRefAction, SqsRefType,
        SqsRepoAction,
    },
};
use sha2::Sha256;
use subtle::ConstantTimeEq;

const GITHUB_WEBHOOK_SECRET: &str = "GITHUB_WEBHOOK_SECRET";

pub struct GitHubEventConverter {
    secret: String,
}

impl EventConverter for GitHubEventConverter {
    fn new(secrets: &Secrets) -> RsWebhookResult<Self> {
        let secret = secrets.get(GITHUB_WEBHOOK_SECRET)?;
        if secret.is_empty() {
            Err(RsSharedError::Config(RsConfigError::empty(
                GITHUB_WEBHOOK_SECRET,
            )))?;
        }
        Ok(Self { secret })
    }

    fn event_response(&self, request: &lambda_http::Request) -> RsWebhookResult<EventResponse> {
        let Some(event_type_str) = request
            .headers()
            .get("x-github-event")
            .and_then(|v| v.to_str().ok())
        else {
            return Ok(EventResponse::WrongGitProvider);
        };
        if !Self::verify_signature(request, &self.secret) {
            return Ok(EventResponse::Unauthorized);
        }
        let delivery_id = Self::delivery_id(request);
        let event_type = GitHubEventType::from(event_type_str);
        tracing::info!(delivery_id = ?delivery_id, "event type: {event_type}");
        match event_type {
            GitHubEventType::Create => {
                self.ref_response(SqsRefAction::Create, request, delivery_id)
            }
            GitHubEventType::Delete => {
                self.ref_response(SqsRefAction::Delete, request, delivery_id)
            }
            GitHubEventType::Installation => self.installation_response(request, delivery_id),
            GitHubEventType::InstallationRepositories => {
                self.installation_repositories_response(request, delivery_id)
            }
            GitHubEventType::Other => self.other_response(),
            GitHubEventType::Ping => self.ping_response(),
            GitHubEventType::Push => self.push_response(request, delivery_id),
            GitHubEventType::Repository => self.repository_response(request, delivery_id),
        }
    }
}

// responses
impl GitHubEventConverter {
    fn installation_response(
        &self,
        request: &lambda_http::Request,
        delivery_id: Option<String>,
    ) -> RsWebhookResult<EventResponse> {
        let event: GitHubEventInstallation =
            serde_json::from_slice(request.body()).map_err(RsWebhookError::Serde)?;
        let installation_id = event.installation.id;
        let host = Self::host(&event.installation)?;
        let action = match event.action {
            GitHubEventInstallationAction::Created => SqsMultiRepoAction::Add,
            GitHubEventInstallationAction::Deleted => SqsMultiRepoAction::Remove,
            _ => return Ok(EventResponse::Ignore),
        };
        if action == SqsMultiRepoAction::Remove {
            return Ok(Self::accept(
                host,
                delivery_id,
                installation_id,
                SqsDetail::AllRepos { action },
            ));
        }
        match event.installation.repository_selection {
            GitHubRepositorySelection::All => Ok(Self::accept(
                host,
                delivery_id,
                installation_id,
                SqsDetail::AllRepos { action },
            )),
            GitHubRepositorySelection::Selected if !event.repositories.is_empty() => {
                Ok(Self::accept(
                    host,
                    delivery_id,
                    installation_id,
                    SqsDetail::MultiRepo {
                        action,
                        repo_ids: Self::repo_ids(event.repositories),
                    },
                ))
            }
            _ => Ok(EventResponse::Ignore),
        }
    }

    fn installation_repositories_response(
        &self,
        request: &lambda_http::Request,
        delivery_id: Option<String>,
    ) -> RsWebhookResult<EventResponse> {
        let event: GitHubEventInstallationRepositories =
            serde_json::from_slice(request.body()).map_err(RsWebhookError::Serde)?;
        let host = Self::host(&event.installation)?;
        let action = match event.action {
            GitHubEventInstallationRepositoriesAction::Added => SqsMultiRepoAction::Add,
            GitHubEventInstallationRepositoriesAction::Removed => SqsMultiRepoAction::Remove,
            GitHubEventInstallationRepositoriesAction::Other => {
                return Ok(EventResponse::Ignore);
            }
        };
        let repositories = match action {
            SqsMultiRepoAction::Add => event.repositories_added,
            SqsMultiRepoAction::Remove => event.repositories_removed,
        };
        if repositories.is_empty() {
            return Ok(EventResponse::Ignore);
        }
        let detail = SqsDetail::MultiRepo {
            action,
            repo_ids: Self::repo_ids(repositories),
        };
        Ok(Self::accept(
            host,
            delivery_id,
            event.installation.id,
            detail,
        ))
    }

    fn repository_response(
        &self,
        request: &lambda_http::Request,
        delivery_id: Option<String>,
    ) -> RsWebhookResult<EventResponse> {
        let event: GitHubEventRepository =
            serde_json::from_slice(request.body()).map_err(RsWebhookError::Serde)?;
        let installation_id = event.installation.id;
        let host = Self::host(&event.installation)?;
        let repo_id = Self::repo_id(event.repository);
        match event.action {
            GitHubEventRepositoryAction::Created => Ok(Self::accept(
                host,
                delivery_id,
                installation_id,
                SqsDetail::Repo {
                    action: SqsRepoAction::Add,
                    repo_id,
                },
            )),
            GitHubEventRepositoryAction::Deleted => Ok(Self::accept(
                host,
                delivery_id,
                installation_id,
                SqsDetail::Repo {
                    action: SqsRepoAction::Remove,
                    repo_id,
                },
            )),
            GitHubEventRepositoryAction::Transferred => Ok(Self::accept(
                host,
                delivery_id,
                installation_id,
                SqsDetail::Repo {
                    action: SqsRepoAction::Transfer,
                    repo_id,
                },
            )),
            GitHubEventRepositoryAction::Archived
            | GitHubEventRepositoryAction::Edited
            | GitHubEventRepositoryAction::Other
            | GitHubEventRepositoryAction::Privatized
            | GitHubEventRepositoryAction::Publicized
            | GitHubEventRepositoryAction::Renamed
            | GitHubEventRepositoryAction::Unarchived => Ok(EventResponse::Ignore),
        }
    }

    fn ref_response(
        &self,
        action: SqsRefAction,
        request: &lambda_http::Request,
        delivery_id: Option<String>,
    ) -> RsWebhookResult<EventResponse> {
        let event: GitHubEventRef =
            serde_json::from_slice(request.body()).map_err(RsWebhookError::Serde)?;
        let host = Self::host(&event.installation)?;
        let Some(ref_type) = Self::ref_type(&event.ref_type) else {
            return Ok(EventResponse::Ignore);
        };
        let detail = SqsDetail::Ref {
            action,
            repo_id: Self::repo_id(event.repository),
            ref_name: Self::ref_name(&event.r#ref),
            ref_type,
        };
        Ok(Self::accept(
            host,
            delivery_id,
            event.installation.id,
            detail,
        ))
    }

    fn other_response(&self) -> RsWebhookResult<EventResponse> {
        Ok(EventResponse::Ignore)
    }

    fn ping_response(&self) -> RsWebhookResult<EventResponse> {
        Ok(EventResponse::Pong)
    }

    fn push_response(
        &self,
        request: &lambda_http::Request,
        delivery_id: Option<String>,
    ) -> RsWebhookResult<EventResponse> {
        let event: GitHubEventPush =
            serde_json::from_slice(request.body()).map_err(RsWebhookError::Serde)?;
        let GitHubEventPush {
            after,
            created,
            deleted,
            forced,
            installation,
            r#ref,
            repository,
            ..
        } = event;
        let host = Self::host(&installation)?;
        if created || deleted || Self::is_zero_sha(&after) {
            return Ok(EventResponse::Ignore);
        }
        let Some(branch_name) = Self::branch_name(&r#ref) else {
            return Ok(EventResponse::Ignore);
        };
        let detail = SqsDetail::Commit {
            repo_id: Self::repo_id(repository),
            branch_name: branch_name.into(),
            commit_hash: after,
            forced,
        };
        Ok(Self::accept(host, delivery_id, installation.id, detail))
    }
}

// helpers
impl GitHubEventConverter {
    fn is_zero_sha(sha: &str) -> bool {
        !sha.is_empty() && sha.bytes().all(|byte| byte == b'0')
    }

    fn repo_ids(repositories: Vec<GitHubRepository>) -> Vec<String> {
        repositories.into_iter().map(Self::repo_id).collect()
    }

    fn repo_id(repository: GitHubRepository) -> String {
        repository.id.to_string()
    }

    fn ref_name(r#ref: &str) -> String {
        r#ref
            .strip_prefix("refs/heads/")
            .or_else(|| r#ref.strip_prefix("refs/tags/"))
            .unwrap_or(r#ref)
            .to_string()
    }

    fn branch_name(r#ref: &str) -> Option<&str> {
        r#ref.strip_prefix("refs/heads/")
    }

    fn ref_type(ref_type: &GitHubRefType) -> Option<SqsRefType> {
        match ref_type {
            GitHubRefType::Branch => Some(SqsRefType::Branch),
            GitHubRefType::Tag => Some(SqsRefType::Tag),
            GitHubRefType::Other => None,
        }
    }

    fn host(installation: &GitHubInstallation) -> RsWebhookResult<String> {
        Self::host_from_url(&installation.html_url)
            .ok_or_else(|| RsWebhookError::InvalidHost(installation.html_url.clone()))
    }

    fn host_from_url(url: &str) -> Option<String> {
        let authority = url
            .strip_prefix("https://")
            .or_else(|| url.strip_prefix("http://"))?
            .split('/')
            .next()?;
        let host = authority.rsplit('@').next()?.split(':').next()?;
        (!host.is_empty()).then(|| host.to_ascii_lowercase())
    }

    fn delivery_id(request: &lambda_http::Request) -> Option<String> {
        request
            .headers()
            .get("x-github-delivery")
            .and_then(|v| v.to_str().ok())
            .filter(|delivery_id| !delivery_id.is_empty())
            .map(str::to_owned)
    }

    fn accept(
        host: String,
        delivery_id: Option<String>,
        installation_id: u64,
        detail: SqsDetail,
    ) -> EventResponse {
        let message = SqsMessage {
            source: GitSource::GitHub {
                host,
                delivery_id,
                installation_id,
            },
            detail,
        };
        EventResponse::Accept(message)
    }

    fn verify_signature(request: &lambda_http::Request, secret: &str) -> bool {
        let signature = request
            .headers()
            .get("x-hub-signature-256")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let private_key = secret.as_bytes();
        let body_bytes = request.body();
        let Some(expected_hex) = signature.strip_prefix("sha256=") else {
            return false;
        };
        let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(private_key) else {
            return false;
        };
        mac.update(body_bytes);
        let computed = mac.finalize().into_bytes();
        let Ok(expected) = hex::decode(expected_hex) else {
            return false;
        };
        computed.ct_eq(&expected).into()
    }
}
