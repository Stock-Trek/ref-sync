use crate::{
    error::SharedResult,
    git_ops::{git_host::GitHost, github::GitHubAuth},
    sqs_message::GitSource,
    timeouts::Deadline,
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
    pub fn from_source(
        github: &GitHubAuth,
        source: &GitSource,
        deadline: Deadline,
    ) -> SharedResult<Self> {
        match source {
            GitSource::GitHub {
                host,
                installation_id,
                ..
            } => Ok(GitRemote(Box::new(github.installation(
                host,
                *installation_id,
                deadline,
            )?))),
        }
    }
}
