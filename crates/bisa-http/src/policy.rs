//! The policy as a value: where the proxy comes from, and the environment a
//! child is handed to obey it.

use std::collections::BTreeMap;
use std::fmt;

use url::Url;

/// The environment variables the platform's tools read a proxy from, both
/// cases — curl and git read the lower-case `http_proxy` only, most of the
/// rest read the upper-case names first.
pub const PROXY_ENV_NAMES: [&str; 8] = [
    "HTTP_PROXY",
    "http_proxy",
    "HTTPS_PROXY",
    "https_proxy",
    "ALL_PROXY",
    "all_proxy",
    "NO_PROXY",
    "no_proxy",
];

/// This machine's own names, bypassed whatever the list says.
pub const LOOPBACK_NAMES: [&str; 3] = ["localhost", "127.0.0.1", "::1"];

/// Where the proxy comes from.
#[derive(Clone, PartialEq, Eq)]
pub enum ProxyPolicy {
    /// What the process environment names, read by reqwest when a client is
    /// built — `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY`, `NO_PROXY`, either
    /// case. Nothing is set for a child and nothing removed: it inherits.
    Environment,
    /// No proxy, whatever the environment says; a child has the names removed.
    None,
    /// The two URLs and the bypass list, for the clients and every child.
    Manual {
        http: Option<Url>,
        https: Option<Url>,
        /// The person's entries; the loopback names are always prepended.
        no_proxy: Vec<String>,
    },
}

impl fmt::Debug for ProxyPolicy {
    /// A password never reaches a log: the manual URLs print masked.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProxyPolicy::Environment => f.write_str("Environment"),
            ProxyPolicy::None => f.write_str("None"),
            ProxyPolicy::Manual {
                http,
                https,
                no_proxy,
            } => f
                .debug_struct("Manual")
                .field("http", &http.as_ref().map(masked))
                .field("https", &https.as_ref().map(masked))
                .field("no_proxy", no_proxy)
                .finish(),
        }
    }
}

/// A URL with its password replaced, the login kept. The same rule as
/// `bisa_core::network_settings::masked`, spelled here because this crate
/// sits below the core in the crate graph: `set_password` percent-encodes the
/// bullets, so the password is dropped and the mask written after the login.
pub(crate) fn masked(url: &Url) -> String {
    if url.password().is_none() {
        return url.to_string();
    }
    let mut shown = url.clone();
    if shown.set_password(None).is_err() {
        return url.to_string();
    }
    let shown = shown.to_string();
    match shown.find('@') {
        Some(at) => format!("{}:\u{2022}\u{2022}\u{2022}{}", &shown[..at], &shown[at..]),
        None => shown,
    }
}

/// How the platform's own HTTP leaves this machine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpPolicy {
    pub proxy: ProxyPolicy,
    /// Every request speaks HTTP/1.1, never HTTP/2.
    pub http1_only: bool,
}

impl Default for HttpPolicy {
    fn default() -> Self {
        Self {
            proxy: ProxyPolicy::Environment,
            http1_only: false,
        }
    }
}

/// What a child the platform runs is handed: names set, then names removed.
/// Removal is what makes `None` mean none — a child would otherwise inherit
/// whatever the node's own environment says.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChildEnv {
    pub set: BTreeMap<String, String>,
    pub remove: Vec<&'static str>,
}

impl HttpPolicy {
    /// The bypass list as the tools read it: this machine's own names first,
    /// then the person's entries, comma-joined. Empty when the policy is not
    /// manual.
    pub fn no_proxy_value(&self) -> String {
        let ProxyPolicy::Manual { no_proxy, .. } = &self.proxy else {
            return String::new();
        };
        LOOPBACK_NAMES
            .iter()
            .map(|s| s.to_string())
            .chain(
                no_proxy
                    .iter()
                    .filter(|e| !LOOPBACK_NAMES.contains(&e.as_str()))
                    .cloned(),
            )
            .collect::<Vec<_>>()
            .join(",")
    }

    /// The environment every child the platform runs is given — the same
    /// rule `bisa_core::NetworkSettings::child_env` states, held equal
    /// by an engine test, because the crates that hold a [`crate::Clients`]
    /// cannot see the core.
    pub fn child_env(&self) -> ChildEnv {
        match &self.proxy {
            ProxyPolicy::Environment => ChildEnv::default(),
            ProxyPolicy::None => ChildEnv {
                set: BTreeMap::new(),
                remove: PROXY_ENV_NAMES.to_vec(),
            },
            ProxyPolicy::Manual { http, https, .. } => {
                let mut set = BTreeMap::new();
                let mut remove = vec!["ALL_PROXY", "all_proxy"];
                match http {
                    Some(url) => {
                        set.insert("HTTP_PROXY".to_string(), url.to_string());
                        set.insert("http_proxy".to_string(), url.to_string());
                    }
                    None => remove.extend(["HTTP_PROXY", "http_proxy"]),
                }
                match https {
                    Some(url) => {
                        set.insert("HTTPS_PROXY".to_string(), url.to_string());
                        set.insert("https_proxy".to_string(), url.to_string());
                    }
                    None => remove.extend(["HTTPS_PROXY", "https_proxy"]),
                }
                let bypass = self.no_proxy_value();
                set.insert("NO_PROXY".to_string(), bypass.clone());
                set.insert("no_proxy".to_string(), bypass);
                ChildEnv { set, remove }
            }
        }
    }

    /// Whether the policy sends anything through a proxy of its own naming.
    pub fn names_a_proxy(&self) -> bool {
        matches!(&self.proxy, ProxyPolicy::Manual { http, https, .. } if http.is_some() || https.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manual(http: Option<&str>, https: Option<&str>, no_proxy: &[&str]) -> HttpPolicy {
        HttpPolicy {
            proxy: ProxyPolicy::Manual {
                http: http.map(|u| Url::parse(u).unwrap()),
                https: https.map(|u| Url::parse(u).unwrap()),
                no_proxy: no_proxy.iter().map(|s| s.to_string()).collect(),
            },
            http1_only: false,
        }
    }

    #[test]
    fn debug_never_shows_a_password() {
        let policy = manual(None, Some("http://ada:s3cret@proxy.example:3128"), &[]);
        let shown = format!("{policy:?}");
        assert!(!shown.contains("s3cret"), "{shown}");
        assert!(shown.contains("ada:\u{2022}\u{2022}\u{2022}@proxy.example:3128"));
    }

    #[test]
    fn the_child_environment_follows_the_policy() {
        assert_eq!(HttpPolicy::default().child_env(), ChildEnv::default());
        let none = HttpPolicy {
            proxy: ProxyPolicy::None,
            http1_only: false,
        };
        assert_eq!(none.child_env().remove, PROXY_ENV_NAMES.to_vec());

        let env = manual(
            None,
            Some("http://u:p@proxy.example:3128"),
            &[".corp.example"],
        )
        .child_env();
        assert_eq!(
            env.set.get("HTTPS_PROXY").map(String::as_str),
            Some("http://u:p@proxy.example:3128/")
        );
        assert_eq!(
            env.set.get("no_proxy").map(String::as_str),
            Some("localhost,127.0.0.1,::1,.corp.example")
        );
        assert_eq!(
            env.remove,
            vec!["ALL_PROXY", "all_proxy", "HTTP_PROXY", "http_proxy"]
        );
    }

    #[test]
    fn a_manual_policy_without_a_url_names_no_proxy() {
        assert!(!manual(None, None, &["a.example"]).names_a_proxy());
        assert!(manual(Some("http://p.example"), None, &[]).names_a_proxy());
        assert!(!HttpPolicy::default().names_a_proxy());
    }
}
