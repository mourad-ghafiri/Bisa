//! The one place a resolved URL is judged against the hosts the definition
//! declares — before a request exists, so a refused host is never dialled.
//! The grammar of a declared host is `bisa-netrules`', the same words the
//! domain validates a definition with and the security rules judge by.

use crate::error::ConnectorError;
use url::Url;

/// The workspace's own say over a declared host: the engine's allow and deny
/// lists, or nothing in a test.
pub trait HostJudge: Send + Sync {
    /// `Err(reason)` refuses the host the definition declared.
    fn judge(&self, host: &str) -> Result<(), String>;
}

/// A judge with no opinion.
#[derive(Clone, Copy, Debug, Default)]
pub struct AllowAll;

impl HostJudge for AllowAll {
    fn judge(&self, _host: &str) -> Result<(), String> {
        Ok(())
    }
}

pub use bisa_netrules::is_loopback;

/// `host[:port]` of a URL, lowercased, the port only when the URL names one.
pub fn host_of(url: &Url) -> Option<String> {
    let host = url.host_str()?.to_ascii_lowercase();
    Some(match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host,
    })
}

/// Refuse a URL the definition does not allow: a scheme that is not `https`
/// (or `http` on loopback), a host not in `declared`, or `insecure_tls` on a
/// host that is not loopback.
pub fn check(url: &Url, declared: &[String], insecure_tls: bool) -> Result<(), ConnectorError> {
    let host = host_of(url)
        .ok_or_else(|| ConnectorError::BadDefinition(format!("{url} names no host")))?;
    let bare = url.host_str().unwrap_or_default();
    match url.scheme() {
        "https" => {}
        "http" if is_loopback(bare) => {}
        "http" => {
            return Err(ConnectorError::HostRefused {
                host: host.clone(),
                allowed: "https only, except on loopback".into(),
            })
        }
        other => {
            return Err(ConnectorError::BadDefinition(format!(
                "scheme {other:?} is not http or https"
            )))
        }
    }
    if insecure_tls && !is_loopback(bare) {
        return Err(ConnectorError::BadDefinition(
            "insecure_tls is allowed for loopback hosts only".into(),
        ));
    }
    if declared
        .iter()
        .any(|d| bisa_netrules::host_matches(d, &host))
    {
        return Ok(());
    }
    Err(ConnectorError::HostRefused {
        host,
        allowed: if declared.is_empty() {
            "the definition declares no host".into()
        } else {
            declared.join(", ")
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn declared_hosts_match_exactly_or_by_wildcard_suffix() {
        let declared = vec![
            "api.example.com".to_string(),
            "*.atlassian.net".to_string(),
            "127.0.0.1:8080".to_string(),
        ];
        assert!(check(&url("https://api.example.com/x"), &declared, false).is_ok());
        assert!(check(&url("https://acme.atlassian.net/rest"), &declared, false).is_ok());
        assert!(matches!(
            check(&url("https://atlassian.net/"), &declared, false),
            Err(ConnectorError::HostRefused { .. })
        ));
        assert!(matches!(
            check(&url("https://evil.example.com/"), &declared, false),
            Err(ConnectorError::HostRefused { .. })
        ));
        assert!(check(&url("http://127.0.0.1:8080/x"), &declared, false).is_ok());
        assert!(matches!(
            check(&url("http://api.example.com/"), &declared, false),
            Err(ConnectorError::HostRefused { .. })
        ));
    }

    #[test]
    fn insecure_tls_is_loopback_only_and_other_schemes_are_bad_definitions() {
        let declared = vec!["api.example.com".to_string(), "localhost:27124".to_string()];
        assert!(matches!(
            check(&url("https://api.example.com/"), &declared, true),
            Err(ConnectorError::BadDefinition(_))
        ));
        assert!(check(&url("https://localhost:27124/"), &declared, true).is_ok());
        assert!(matches!(
            check(&url("ftp://api.example.com/"), &declared, false),
            Err(ConnectorError::BadDefinition(_))
        ));
        assert!(is_loopback("[::1]"));
        assert!(!is_loopback("example.com"));
    }
}
