use crate::sqs_message::GitSource;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumIter};

#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, Hash, EnumIter, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GitProvider {
    GitHub,
}

impl From<GitSource> for GitProvider {
    fn from(value: GitSource) -> Self {
        match value {
            GitSource::GitHub { .. } => GitProvider::GitHub,
        }
    }
}
