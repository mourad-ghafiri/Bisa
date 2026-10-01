//! One swappable set of clients, built from a policy.

use std::sync::{Arc, RwLock};
use std::time::Duration;

use reqwest::{Client, ClientBuilder, NoProxy, Proxy};

use crate::policy::{masked, ChildEnv, HttpPolicy, ProxyPolicy};

/// How long a connection may take to open. A request's own deadline is the
/// caller's, per request.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const USER_AGENT: &str = "bisa";

/// A policy that could not become a client, with the reason reqwest gave.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct HttpError(String);

impl HttpError {
    fn from_reqwest(e: reqwest::Error) -> Self {
        // reqwest's message can carry a URL, and a proxy URL can carry a
        // login; the policy's own `Debug` masks, the error names the cause.
        HttpError(e.to_string())
    }
}

struct Set {
    outbound: Arc<Client>,
    strict: Arc<Client>,
}

/// The clients every crate reaches the network through, and the policy
/// behind them. `apply` swaps the set whole; a handle taken before a swap
/// finishes its request on the old one.
pub struct Clients {
    policy: RwLock<HttpPolicy>,
    set: RwLock<Set>,
    loopback: Arc<Client>,
}

impl std::fmt::Debug for Clients {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Clients")
            .field("policy", &self.policy())
            .finish_non_exhaustive()
    }
}

impl Default for Clients {
    /// The default policy — the environment's proxy, if any — with the
    /// contract `reqwest::Client::new()` already has: building it cannot
    /// fail on a machine with a TLS backend.
    fn default() -> Self {
        Self::new(&HttpPolicy::default()).expect("the default HTTP clients build")
    }
}

impl Clients {
    /// The process's one default set — the environment's proxy, if any — for
    /// a caller that was handed no policy: a test's stub server, a one-shot
    /// command with no workspace open. The engine hands every crate its own
    /// handle, built from the `network.*` settings, and this one is never
    /// rebuilt.
    pub fn shared() -> Arc<Clients> {
        static SHARED: std::sync::OnceLock<Arc<Clients>> = std::sync::OnceLock::new();
        Arc::clone(SHARED.get_or_init(|| Arc::new(Clients::default())))
    }

    pub fn new(policy: &HttpPolicy) -> Result<Self, HttpError> {
        let set = build(policy)?;
        let loopback = Self::loopback_builder()
            .build()
            .map_err(HttpError::from_reqwest)?;
        Ok(Self {
            policy: RwLock::new(policy.clone()),
            set: RwLock::new(set),
            loopback: Arc::new(loopback),
        })
    }

    /// Replace the clients with ones built from `policy`. The new set is built
    /// first; a failure keeps the old one and answers why.
    pub fn apply(&self, policy: &HttpPolicy) -> Result<(), HttpError> {
        let set = build(policy)?;
        *self.set.write().unwrap_or_else(|p| p.into_inner()) = set;
        *self.policy.write().unwrap_or_else(|p| p.into_inner()) = policy.clone();
        Ok(())
    }

    /// The policy the clients were built from.
    pub fn policy(&self) -> HttpPolicy {
        self.policy
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    /// The client for the internet: the policy's proxy, redirects followed.
    pub fn outbound(&self) -> Arc<Client> {
        Arc::clone(&self.set.read().unwrap_or_else(|p| p.into_inner()).outbound)
    }

    /// The client for a call that must not follow a redirect — a connector
    /// step, whose declared host a redirect could leave.
    pub fn strict(&self) -> Arc<Client> {
        Arc::clone(&self.set.read().unwrap_or_else(|p| p.into_inner()).strict)
    }

    /// The client for a service on this machine: never a proxy, HTTP/1.1.
    pub fn loopback(&self) -> Arc<Client> {
        Arc::clone(&self.loopback)
    }

    /// The loopback client's builder, for a caller that needs one more
    /// option on it — a self-signed certificate a loopback service serves.
    pub fn loopback_builder() -> ClientBuilder {
        base().no_proxy().http1_only()
    }

    /// The environment a child is handed to obey the same policy.
    pub fn child_env(&self) -> ChildEnv {
        self.policy().child_env()
    }
}

fn base() -> ClientBuilder {
    Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(CONNECT_TIMEOUT)
}

/// The policy on a builder: the proxy, then the version.
fn configure(policy: &HttpPolicy, mut builder: ClientBuilder) -> Result<ClientBuilder, HttpError> {
    match &policy.proxy {
        // reqwest reads the environment itself when the client is built.
        ProxyPolicy::Environment => {}
        ProxyPolicy::None => builder = builder.no_proxy(),
        ProxyPolicy::Manual { http, https, .. } => {
            let bypass = NoProxy::from_string(&policy.no_proxy_value());
            if let Some(url) = http {
                let proxy = Proxy::http(url.clone())
                    .map_err(|e| HttpError(format!("HTTP proxy {}: {e}", masked(url))))?;
                builder = builder.proxy(proxy.no_proxy(bypass.clone()));
            }
            if let Some(url) = https {
                let proxy = Proxy::https(url.clone())
                    .map_err(|e| HttpError(format!("HTTPS proxy {}: {e}", masked(url))))?;
                builder = builder.proxy(proxy.no_proxy(bypass.clone()));
            }
            // A manual policy naming no URL is direct — and says so to
            // reqwest, so the environment is not read behind it.
            if http.is_none() && https.is_none() {
                builder = builder.no_proxy();
            }
        }
    }
    if policy.http1_only {
        builder = builder.http1_only();
    }
    Ok(builder)
}

fn build(policy: &HttpPolicy) -> Result<Set, HttpError> {
    let outbound = configure(policy, base())?
        .build()
        .map_err(HttpError::from_reqwest)?;
    let strict = configure(policy, base().redirect(reqwest::redirect::Policy::none()))?
        .build()
        .map_err(HttpError::from_reqwest)?;
    Ok(Set {
        outbound: Arc::new(outbound),
        strict: Arc::new(strict),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use url::Url;

    fn manual(https: &str) -> HttpPolicy {
        HttpPolicy {
            proxy: ProxyPolicy::Manual {
                http: None,
                https: Some(Url::parse(https).unwrap()),
                no_proxy: vec![".corp.example".into()],
            },
            http1_only: true,
        }
    }

    #[test]
    fn every_policy_builds_and_apply_swaps_the_policy() {
        let clients = Clients::default();
        assert_eq!(clients.policy(), HttpPolicy::default());
        clients
            .apply(&manual("http://ada:s3cret@proxy.example:3128"))
            .unwrap();
        assert!(clients.policy().http1_only);
        assert!(clients.policy().names_a_proxy());
        clients
            .apply(&HttpPolicy {
                proxy: ProxyPolicy::None,
                http1_only: false,
            })
            .unwrap();
        assert!(!clients.policy().names_a_proxy());
        let shown = format!("{clients:?}");
        assert!(!shown.contains("s3cret"));
    }

    #[test]
    fn the_loopback_client_is_one_and_the_same_across_a_swap() {
        let clients = Clients::default();
        let before = clients.loopback();
        clients.apply(&manual("http://proxy.example:3128")).unwrap();
        assert!(Arc::ptr_eq(&before, &clients.loopback()));
    }

    #[test]
    fn a_manual_policy_without_a_url_is_direct() {
        let policy = HttpPolicy {
            proxy: ProxyPolicy::Manual {
                http: None,
                https: None,
                no_proxy: vec![],
            },
            http1_only: false,
        };
        assert!(Clients::new(&policy).is_ok());
        assert!(!policy.names_a_proxy());
    }
}
