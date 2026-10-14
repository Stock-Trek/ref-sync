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
use url::{Host, Url};

pub const DEFAULT_HOST: &str = "github.com";
const ENTERPRISE_ALLOWED_HOSTS: &str = "GITHUB_ENTERPRISE_ALLOWED_HOSTS";
const DEFAULT_HTTPS_PORT: u16 = 443;

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

    /// Builds an allowlist from an explicit set of hosts. Useful for tests and
    /// for embedders that resolve the host list from somewhere other than the
    /// environment.
    pub fn from_hosts<I, S>(hosts: I) -> RsSharedResult<Self>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut set = HashSet::new();
        for host in hosts {
            set.insert(Self::validated(host.as_ref())?);
        }
        Ok(Self { hosts: set })
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
            Some(port) if port != DEFAULT_HTTPS_PORT => Some(format!("{name}:{port}")),
            _ => Some(name),
        }
    }

    /// Parses a URL from an untrusted source (such as an `html_url` field on a
    /// GitHub webhook) and returns its normalized host if it is safe to use for
    /// an outbound request.
    ///
    /// The URL is parsed with a real URL parser. The scheme must be `https`,
    /// the port must be absent or `443`, userinfo is rejected, and the host is
    /// lowercased and checked against the allowlist. Callers must rebuild the
    /// request URL from the returned host rather than reusing the original
    /// string.
    pub fn validate_url(&self, url: &str) -> Result<String, RsHostError> {
        let parsed = Url::parse(url).map_err(|_| RsHostError::Invalid {
            host: url.to_owned(),
        })?;
        let host = Self::host_from_url(&parsed).ok_or_else(|| RsHostError::Invalid {
            host: url.to_owned(),
        })?;
        if self.hosts.contains(&host) {
            Ok(host)
        } else {
            Err(RsHostError::NotAllowed {
                host: url.to_owned(),
            })
        }
    }

    fn host_from_url(url: &Url) -> Option<String> {
        if url.scheme() != "https" {
            return None;
        }
        if !matches!(url.port(), None | Some(DEFAULT_HTTPS_PORT)) {
            return None;
        }
        if !url.username().is_empty() || url.password().is_some() {
            return None;
        }
        match url.host()? {
            Host::Domain(name) => Self::normalize_name(name),
            Host::Ipv4(address) => Some(address.to_string()),
            Host::Ipv6(address) => Some(format!("[{address}]")),
        }
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

    fn allowlist() -> HostAllowlist {
        HostAllowlist {
            hosts: HashSet::from([DEFAULT_HOST.to_owned(), "ghe.example.com".to_owned()]),
        }
    }

    #[test]
    fn accepts_https_allowlisted_host() {
        assert_eq!(
            allowlist()
                .validate_url("https://github.com/Stock-Trek/ref-sync")
                .unwrap(),
            "github.com"
        );
    }

    #[test]
    fn accepts_enterprise_host_and_explicit_https_port() {
        assert_eq!(
            allowlist()
                .validate_url("https://ghe.example.com/")
                .unwrap(),
            "ghe.example.com"
        );
        assert_eq!(
            allowlist()
                .validate_url("https://ghe.example.com:443/")
                .unwrap(),
            "ghe.example.com"
        );
    }

    #[test]
    fn lowercases_uppercase_host() {
        assert_eq!(
            allowlist().validate_url("https://GITHUB.COM/").unwrap(),
            "github.com"
        );
        assert_eq!(
            allowlist().validate_url("HTTPS://GitHub.Com/").unwrap(),
            "github.com"
        );
    }

    #[test]
    fn rejects_non_https_schemes() {
        for url in [
            "http://github.com/",
            "ftp://github.com/",
            "file:///etc/passwd",
            "ws://github.com/",
            "//github.com/",
        ] {
            assert!(allowlist().validate_url(url).is_err(), "accepted {url}");
        }
    }

    #[test]
    fn rejects_non_allowlisted_host() {
        assert!(matches!(
            allowlist().validate_url("https://evil.com/"),
            Err(RsHostError::NotAllowed { .. })
        ));
    }

    #[test]
    fn rejects_non_443_ports() {
        for url in [
            "https://github.com:80/",
            "https://github.com:8443/",
            "https://github.com:22/",
        ] {
            assert!(allowlist().validate_url(url).is_err(), "accepted {url}");
        }
    }

    #[test]
    fn rejects_userinfo_tricks() {
        for url in [
            "https://github.com@evil.com/",
            "https://evil.com@github.com/",
            "https://user:secret@github.com/",
        ] {
            assert!(allowlist().validate_url(url).is_err(), "accepted {url}");
        }
    }

    #[test]
    fn rejects_percent_encoded_non_allowlisted_host() {
        assert!(allowlist().validate_url("https://evil%2Ecom/").is_err());
    }

    #[test]
    fn normalizes_percent_encoded_allowlisted_host() {
        assert_eq!(
            allowlist().validate_url("https://%67ithub.com/").unwrap(),
            "github.com"
        );
    }

    #[test]
    fn validate_lowercases_and_strips_default_port() {
        assert_eq!(
            allowlist().validate("GITHUB.COM:443").unwrap(),
            "github.com"
        );
    }

    #[test]
    fn validate_rejects_non_default_port() {
        assert!(allowlist().validate("github.com:80").is_err());
        assert!(allowlist().validate("github.com:8443").is_err());
    }

    #[test]
    fn validate_allows_configured_enterprise_port() {
        let mut hosts = HashSet::from([DEFAULT_HOST.to_owned()]);
        hosts.insert("ghe.example.com:8443".to_owned());
        let allowlist = HostAllowlist { hosts };
        assert_eq!(
            allowlist.validate("ghe.example.com:8443").unwrap(),
            "ghe.example.com:8443"
        );
    }
}
