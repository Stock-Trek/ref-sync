use crate::{
    error::RsSharedResult,
    git_ops::{git_host::GitHost, github::github_auth::GitHubAuth},
    sqs_message::GitSource,
};
use std::ops::Deref;

pub struct GitRemote(Box<dyn GitHost>);

impl Deref for GitRemote {
    type Target = dyn GitHost;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

impl GitRemote {
    pub fn from_source(github_auth: &GitHubAuth, source: &GitSource) -> RsSharedResult<Self> {
        match source {
            GitSource::GitHub {
                host,
                installation_id,
                ..
            } => Ok(GitRemote(Box::new(
                github_auth.installation(host, *installation_id)?,
            ))),
        }
    }
}
