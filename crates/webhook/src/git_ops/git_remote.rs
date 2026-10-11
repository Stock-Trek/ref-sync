use crate::{context::GwhContext, error::RsWebhookResult, git_ops::git_host::GitHost};
use ref_sync_shared::sqs_message::GitSource;
use std::ops::Deref;

pub struct GitRemote(Box<dyn GitHost>);

impl Deref for GitRemote {
    type Target = dyn GitHost;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

impl GitRemote {
    pub fn from_source(context: &GwhContext, source: &GitSource) -> RsWebhookResult<Self> {
        match source {
            GitSource::GitHub {
                installation_id, ..
            } => Ok(GitRemote(Box::new(
                context.github.installation(*installation_id)?,
            ))),
        }
    }
}
