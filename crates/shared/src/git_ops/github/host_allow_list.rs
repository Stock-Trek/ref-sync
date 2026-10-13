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
                format!(
                    "{host:?} is not a hostname with an optional port (expected e.g. \"ghe.example.com\" or \"ghe.example.com:8443\")"
                ),
            )));
        }
        Ok(Self::normalize(host))
    }

    /// Returns whether `host` is a DNS hostname or IPv4 address with an
    /// optional port, i.e. a value that can match the authority extracted from
    /// a GitHub webhook URL.
    fn is_valid_host(host: &str) -> bool {
        if host.is_empty() || host.len() > 259 {
            return false;
        }
        if host
            .chars()
            .any(|c| c.is_whitespace() || matches!(c, '/' | '@' | '?' | '#' | '[' | ']'))
        {
            return false;
        }
        let (name, port) = match host.rsplit_once(':') {
            Some((name, port)) => (name, Some(port)),
            None => (host, None),
        };
        if let Some(port) = port {
            if port.is_empty() || !port.bytes().all(|byte| byte.is_ascii_digit()) {
                return false;
            }
            if port.parse::<u16>().map_or(true, |port| port == 0) {
                return false;
            }
        }
        name.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
    }
}

#[cfg(test)]
mod tests {
    use super::HostAllowlist;

    #[test]
    fn accepts_hostnames_with_optional_ports() {
        assert!(HostAllowlist::is_valid_host("ghe.example.com"));
        assert!(HostAllowlist::is_valid_host("ghe.example.com:8443"));
        assert!(HostAllowlist::is_valid_host("192.168.0.1:8080"));
    }

    #[test]
    fn rejects_invalid_hosts() {
        assert!(!HostAllowlist::is_valid_host(""));
        assert!(!HostAllowlist::is_valid_host("https://ghe.example.com"));
        assert!(!HostAllowlist::is_valid_host("ghe.example.com:"));
        assert!(!HostAllowlist::is_valid_host("ghe.example.com:0"));
        assert!(!HostAllowlist::is_valid_host("ghe.example.com:99999"));
        assert!(!HostAllowlist::is_valid_host("ghe.example.com:abc"));
        assert!(!HostAllowlist::is_valid_host("-ghe.example.com"));
    }
}
