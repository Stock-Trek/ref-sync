use crate::{
    env_vars::EnvVars,
    errors::{
        config::RsConfigError,
        error::{RsSharedError, RsSharedResult},
    },
};
use std::{
    collections::HashSet,
    net::{Ipv4Addr, Ipv6Addr},
};

pub const DEFAULT_HOST: &str = "github.com";
const ENTERPRISE_ALLOWED_HOSTS: &str = "GITHUB_ENTERPRISE_ALLOWED_HOSTS";
const DEFAULT_PORTS: [u16; 2] = [80, 443];

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RsHostError {
    #[error("{host:?} is not a hostname, IP address, or host:port")]
    Invalid { host: String },
    #[error("host {host:?} is not in the allowed GitHub hosts list")]
    NotAllowed { host: String },
}

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
        self.validate(host).is_ok()
    }

    pub fn validate(&self, host: &str) -> Result<String, RsHostError> {
        let normalized = Self::normalize(host).ok_or_else(|| RsHostError::Invalid {
            host: host.to_owned(),
        })?;
        if self.hosts.contains(&normalized) {
            Ok(normalized)
        } else {
            Err(RsHostError::NotAllowed {
                host: host.to_owned(),
            })
        }
    }

    pub fn normalize(host: &str) -> Option<String> {
        let host = host.trim();
        if host.is_empty() {
            return None;
        }
        let (name, port) = Self::split_port(host)?;
        let name = Self::normalize_name(name)?;
        match port {
            Some(port) if !DEFAULT_PORTS.contains(&port) => Some(format!("{name}:{port}")),
            _ => Some(name),
        }
    }

    pub fn from_url(url: &str) -> Option<String> {
        let (scheme, rest) = url.split_once("://")?;
        if !matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https") {
            return None;
        }
        let authority = rest.split(['/', '?', '#']).next()?;
        let authority = authority
            .rsplit_once('@')
            .map_or(authority, |(_, host)| host);
        Self::normalize(authority)?;
        Some(authority.to_owned())
    }

    fn validated(host: &str) -> RsSharedResult<String> {
        Self::normalize(host).ok_or_else(|| {
            RsSharedError::Config(RsConfigError::invalid(
                ENTERPRISE_ALLOWED_HOSTS,
                format!(
                    "{host:?} is not a hostname, IP address, or host:port (expected e.g. \"ghe.example.com\", \"ghe.example.com:8443\", or \"[2001:db8::1]:8443\")"
                ),
            ))
        })
    }

    fn split_port(authority: &str) -> Option<(&str, Option<u16>)> {
        if let Some(bracketed) = authority.strip_prefix('[') {
            let (name, rest) = bracketed.split_once(']')?;
            let port = match rest {
                "" => None,
                rest => Some(Self::parse_port(rest.strip_prefix(':')?)?),
            };
            name.parse::<Ipv6Addr>().ok()?;
            return Some((name, port));
        }
        if authority.matches(':').count() > 1 {
            authority.parse::<Ipv6Addr>().ok()?;
            return Some((authority, None));
        }
        match authority.rsplit_once(':') {
            Some((name, port)) => Some((name, Some(Self::parse_port(port)?))),
            None => Some((authority, None)),
        }
    }

    fn parse_port(port: &str) -> Option<u16> {
        let port: u16 = port.parse().ok()?;
        (port != 0).then_some(port)
    }

    fn normalize_name(name: &str) -> Option<String> {
        if name.contains(':') {
            let address: Ipv6Addr = name.parse().ok()?;
            return Some(format!("[{address}]"));
        }
        if let Ok(address) = name.parse::<Ipv4Addr>() {
            return Some(address.to_string());
        }
        let name = name.trim_end_matches('.').to_ascii_lowercase();
        if name.is_empty() || name.len() > 253 {
            return None;
        }
        name.split('.')
            .all(|label| {
                !label.is_empty()
                    && label.len() <= 63
                    && !label.starts_with('-')
                    && !label.ends_with('-')
                    && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
            })
            .then_some(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allowlist(hosts: &[&str]) -> HostAllowlist {
        HostAllowlist {
            hosts: hosts.iter().map(|host| (*host).to_owned()).collect(),
        }
    }

    #[test]
    fn from_url_preserves_the_reported_host() {
        assert_eq!(
            HostAllowlist::from_url("https://GitHub.COM/settings/installations/1"),
            Some("GitHub.COM".to_owned())
        );
        assert_eq!(
            HostAllowlist::from_url("https://ghe.example.com:443/settings"),
            Some("ghe.example.com:443".to_owned())
        );
        assert_eq!(
            HostAllowlist::from_url("https://[2001:db8::1]:8443/settings"),
            Some("[2001:db8::1]:8443".to_owned())
        );
    }

    #[test]
    fn from_url_rejects_unparseable_urls() {
        assert_eq!(HostAllowlist::from_url("ftp://github.com"), None);
        assert_eq!(HostAllowlist::from_url("not a url"), None);
        assert_eq!(HostAllowlist::from_url("https:///settings"), None);
    }

    #[test]
    fn validate_distinguishes_invalid_from_not_allowed() {
        let allowlist = allowlist(&["github.com"]);
        assert_eq!(
            allowlist.validate("GitHub.com"),
            Ok("github.com".to_owned())
        );
        assert_eq!(
            allowlist.validate("not a host"),
            Err(RsHostError::Invalid {
                host: "not a host".to_owned()
            })
        );
        assert_eq!(
            allowlist.validate("evil.example.com"),
            Err(RsHostError::NotAllowed {
                host: "evil.example.com".to_owned()
            })
        );
    }

    #[test]
    fn validate_normalizes_equivalent_hosts() {
        let allowlist = allowlist(&["ghe.example.com"]);
        assert_eq!(
            allowlist.validate("ghe.example.com:443"),
            Ok("ghe.example.com".to_owned())
        );
        assert_eq!(
            allowlist.validate("GHE.Example.COM."),
            Ok("ghe.example.com".to_owned())
        );
    }
}
