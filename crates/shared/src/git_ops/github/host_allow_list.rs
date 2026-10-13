use crate::{
    env_vars::EnvVars,
    errors::{
        config::RsConfigError,
        error::{RsSharedError, RsSharedResult},
    },
};
use std::collections::HashSet;

pub const DEFAULT_HOST: &str = "github.com";
const ENTERPRISE_ALLOWED_HOSTS: &str = "GITHUB_ENTERPRISE_ALLOWED_HOSTS";

#[derive(Debug, Clone)]
pub struct HostAllowlist {
    hosts: HashSet<String>,
}

impl HostAllowlist {
    pub fn from_env() -> RsSharedResult<Self> {
        let mut hosts = HashSet::from([DEFAULT_HOST.to_owned()]);
        for host in EnvVars::optional_csv_env_var(ENTERPRISE_ALLOWED_HOSTS)? {
            hosts.insert(Self::validated(&host)?);
        }
        Ok(Self { hosts })
    }

    pub fn allows(&self, host: &str) -> bool {
        self.hosts.contains(&Self::normalize(host))
    }

    pub fn normalize(host: &str) -> String {
        host.to_ascii_lowercase()
    }
}

impl HostAllowlist {
    fn validated(host: &str) -> RsSharedResult<String> {
        if !Self::is_valid_host(host) {
            return Err(RsSharedError::Config(RsConfigError::invalid(
                ENTERPRISE_ALLOWED_HOSTS,
                format!("{host:?} is not a bare hostname (expected e.g. \"ghe.example.com\")"),
            )));
        }
        Ok(Self::normalize(host))
    }

    /// Returns whether `host` is a bare DNS hostname or IPv4 address, i.e. a
    /// value that can match the host extracted from a GitHub webhook URL.
    fn is_valid_host(host: &str) -> bool {
        if host.is_empty() || host.len() > 253 {
            return false;
        }
        if host
            .chars()
            .any(|c| c.is_whitespace() || matches!(c, '/' | ':' | '@' | '?' | '#' | '[' | ']'))
        {
            return false;
        }
        host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
    }
}
