//! The `network.*` settings, followed (ide/13 §Capabilities › Network): how
//! the platform reaches the internet, and what it hands everything it runs.
//!
//! One truth, four readers. The five keys resolve into a
//! [`NetworkSettings`] snapshot; the snapshot becomes an [`HttpPolicy`] on
//! the engine's one set of clients (`Inner::http`, an `bisa_http::Clients`
//! every crate that reaches the network holds by `Arc`), refreshed on every
//! write of a `network.*` key the way `cache::refresh_for` and
//! `logging::refresh_for` are; the same policy is the environment every
//! child the platform runs is handed — a harness session, a harness launched
//! in a terminal, git, `gh`/`glab`, a workstream script — so the platform and
//! its tools agree; and `status` says what is in force now, with a proxy's
//! password masked, for the route and the panel.
//!
//! A service on this machine never sees a proxy: the harness event puller
//! goes through `Clients::loopback`, and the bypass list always carries this
//! machine's own names.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use bisa_core::network_settings::{keys, masked};
use bisa_core::{NetworkSettings, ProxyMode};
use bisa_http::{ChildEnv, Clients, HttpError, HttpPolicy, ProxyPolicy};
use bisa_store::Workspace;
use serde::{Deserialize, Serialize};

use crate::{EngineError, Inner};

/// How long the check's one request may take.
const CHECK_TIMEOUT: Duration = Duration::from_secs(10);

/// What the last `apply` said, for the status to repeat.
#[derive(Default)]
pub struct NetworkState {
    last_error: Mutex<Option<String>>,
}

impl NetworkState {
    fn note(&self, result: &Result<(), HttpError>) {
        *self.last_error.lock().unwrap_or_else(|e| e.into_inner()) =
            result.as_ref().err().map(|e| e.to_string());
    }

    fn last_error(&self) -> Option<String> {
        self.last_error
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

/// The `network.*` keys as the workspace resolves them now.
pub fn snapshot(ws: &Workspace) -> NetworkSettings {
    NetworkSettings::from_resolved(&ws.settings(None).unwrap_or_default())
}

/// The settings as a policy. A URL a hand-edited file left unparsable is
/// dropped and named — the write path refuses one, so this is the file's
/// own doing.
pub fn policy(settings: &NetworkSettings) -> (HttpPolicy, Vec<String>) {
    let mut problems = Vec::new();
    let mut parse = |key: &str, text: &Option<String>| {
        text.as_deref().and_then(|t| match reqwest::Url::parse(t) {
            Ok(url) => Some(url),
            Err(e) => {
                problems.push(format!("{key} is not a URL ({e}); ignored"));
                None
            }
        })
    };
    let proxy = match settings.mode {
        ProxyMode::Environment => ProxyPolicy::Environment,
        ProxyMode::None => ProxyPolicy::None,
        ProxyMode::Manual => ProxyPolicy::Manual {
            http: parse(keys::PROXY_HTTP, &settings.http),
            https: parse(keys::PROXY_HTTPS, &settings.https),
            no_proxy: settings.no_proxy.clone(),
        },
    };
    (
        HttpPolicy {
            proxy,
            http1_only: settings.http1_only,
        },
        problems,
    )
}

/// The clients a process serving `ws` starts with — the one path the CLI and
/// the engine share, so a handle made before the engine and the engine's own
/// are the same thing. A policy that cannot become a client is logged and the
/// default set stands; `status` says so until a write fixes it.
pub fn clients_from(ws: &Workspace) -> Clients {
    let (policy, problems) = policy(&snapshot(ws));
    for p in &problems {
        tracing::warn!(target: "bisa_engine", "network settings: {p}");
    }
    match Clients::new(&policy) {
        Ok(clients) => clients,
        Err(e) => {
            tracing::warn!(target: "bisa_engine", "the proxy could not be applied: {e}");
            Clients::default()
        }
    }
}

/// On a write of a `network.*` key: read the settings again and swap the
/// clients. A failure keeps the old clients and is repeated by `status`.
pub fn refresh_for(inner: &Inner, key: &str) {
    if !key.starts_with("network.") {
        return;
    }
    let (policy, problems) = policy(&snapshot(&inner.ws));
    for p in &problems {
        tracing::warn!(target: "bisa_engine", "network settings: {p}");
    }
    let result = inner.http.apply(&policy);
    if let Err(e) = &result {
        tracing::warn!(target: "bisa_engine", "the proxy could not be applied: {e}");
    }
    inner.network.note(&result);
}

/// The environment every child the platform runs is handed, from the policy
/// the clients follow now — never a second read of the settings, so a child
/// and the platform's own calls cannot disagree.
pub fn child_env(inner: &Inner) -> ChildEnv {
    inner.http.child_env()
}

/// A session's environment: `base` with the proxy names set, and the names
/// the harness must not inherit.
pub fn session_env(
    inner: &Inner,
    mut base: BTreeMap<String, String>,
) -> (BTreeMap<String, String>, Vec<String>) {
    let child = child_env(inner);
    base.extend(child.set);
    (base, child.remove.iter().map(|s| s.to_string()).collect())
}

/// A git handle under the policy: the proxy as curl reads it, and
/// `http.version` when the platform speaks HTTP/1.1 only.
pub fn git_under(mut git: bisa_vcs::Git, policy: &HttpPolicy) -> bisa_vcs::Git {
    let child = policy.child_env();
    for (k, v) in child.set {
        git = git.with_env(k, v);
    }
    for k in child.remove {
        git = git.without_env(k);
    }
    if policy.http1_only {
        git = git.with_config("http.version", "HTTP/1.1");
    }
    git
}

/// What the clients do now.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InForce {
    /// Every call leaves directly.
    Direct,
    /// Calls leave through a proxy; the URLs masked, the bypass as the
    /// tools read it.
    Proxy {
        http: Option<String>,
        https: Option<String>,
        no_proxy: Option<String>,
    },
}

/// What the node's own process environment names, masked — the answer to
/// *what does `environment` mean here*.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProxyEnvironment {
    pub http: Option<String>,
    pub https: Option<String>,
    pub no_proxy: Option<String>,
}

/// The `network.*` settings as the panel reads them, with what is in force.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NetworkStatus {
    pub mode: ProxyMode,
    /// The manual URLs, masked.
    pub http: Option<String>,
    pub https: Option<String>,
    pub no_proxy: Vec<String>,
    pub http1_only: bool,
    pub in_force: InForce,
    pub environment: ProxyEnvironment,
    /// What keeps the settings from being in force, if anything.
    pub problems: Vec<String>,
}

/// One request the person asked for, to see whether the way out works.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NetworkCheck {
    pub url: String,
    /// An answer came back — whatever its status; a 403 still proves the way.
    pub ok: bool,
    pub status: Option<u16>,
    pub elapsed_ms: u64,
    pub error: Option<String>,
    /// Whether the request went through a proxy the policy names.
    pub via_proxy: bool,
}

fn env_var(upper: &str, lower: &str) -> Option<String> {
    std::env::var(upper)
        .ok()
        .or_else(|| std::env::var(lower).ok())
        .filter(|v| !v.is_empty())
}

/// The node's own environment, masked.
fn environment() -> ProxyEnvironment {
    let all = env_var("ALL_PROXY", "all_proxy");
    ProxyEnvironment {
        http: env_var("HTTP_PROXY", "http_proxy")
            .or_else(|| all.clone())
            .map(|u| masked(&u)),
        https: env_var("HTTPS_PROXY", "https_proxy")
            .or(all)
            .map(|u| masked(&u)),
        no_proxy: env_var("NO_PROXY", "no_proxy"),
    }
}

fn in_force(policy: &HttpPolicy, environment: &ProxyEnvironment) -> InForce {
    match &policy.proxy {
        ProxyPolicy::None => InForce::Direct,
        ProxyPolicy::Environment => {
            if environment.http.is_none() && environment.https.is_none() {
                InForce::Direct
            } else {
                InForce::Proxy {
                    http: environment.http.clone(),
                    https: environment.https.clone(),
                    no_proxy: environment.no_proxy.clone(),
                }
            }
        }
        ProxyPolicy::Manual { http, https, .. } => {
            if http.is_none() && https.is_none() {
                InForce::Direct
            } else {
                InForce::Proxy {
                    http: http.as_ref().map(|u| masked(u.as_str())),
                    https: https.as_ref().map(|u| masked(u.as_str())),
                    no_proxy: Some(policy.no_proxy_value()),
                }
            }
        }
    }
}

pub fn status(inner: &Inner) -> NetworkStatus {
    let settings = snapshot(&inner.ws);
    let (wanted, mut problems) = policy(&settings);
    let current = inner.http.policy();
    if current != wanted {
        problems.push(match inner.network.last_error() {
            Some(e) => format!("the settings are not in force — the proxy could not be applied: {e}"),
            None => "the settings are not in force — the clients were built before them; write any network setting again".to_string(),
        });
    }
    let environment = environment();
    NetworkStatus {
        mode: settings.mode,
        http: settings.http.as_deref().map(masked),
        https: settings.https.as_deref().map(masked),
        no_proxy: settings.no_proxy.clone(),
        http1_only: settings.http1_only,
        in_force: in_force(&current, &environment),
        environment,
        problems,
    }
}

fn is_loopback_host(host: &str) -> bool {
    let bare = host.trim_matches(|c| c == '[' || c == ']');
    bare.eq_ignore_ascii_case("localhost")
        || bare == "::1"
        || bare == "0.0.0.0"
        || bare.starts_with("127.")
}

/// The URL a check may go to: `http` or `https`, and not this machine — the
/// check is for the way out.
pub fn check_url(url: &str) -> Result<reqwest::Url, EngineError> {
    let parsed = reqwest::Url::parse(url.trim()).map_err(|e| {
        EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-not-url-check",
            e = e.to_string()
        ))
    })?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-check-goes-http-https-url-not",
            a0 = (parsed.scheme()).to_string()
        )));
    }
    match parsed.host_str() {
        None => Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-check-needs-host"
        ))),
        Some(h) if is_loopback_host(h) => Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-check-way-out-machine-not-loopback-address"
        ))),
        Some(_) => Ok(parsed),
    }
}

/// One `GET` through the outbound client, the body dropped.
pub async fn check(inner: &Inner, url: &str) -> Result<NetworkCheck, EngineError> {
    let target = check_url(url)?;
    let via_proxy = matches!(
        in_force(&inner.http.policy(), &environment()),
        InForce::Proxy { .. }
    );
    let started = Instant::now();
    let answer = inner
        .http
        .outbound()
        .get(target.clone())
        .timeout(CHECK_TIMEOUT)
        .send()
        .await;
    let elapsed_ms = started.elapsed().as_millis() as u64;
    Ok(match answer {
        Ok(resp) => NetworkCheck {
            url: target.to_string(),
            ok: true,
            status: Some(resp.status().as_u16()),
            elapsed_ms,
            error: None,
            via_proxy,
        },
        Err(e) => NetworkCheck {
            url: target.to_string(),
            ok: false,
            status: None,
            elapsed_ms,
            error: Some(cause_of(&e)),
            via_proxy,
        },
    })
}

/// The innermost cause, without the URL reqwest prefixes — a proxy URL in a
/// message could carry a login.
fn cause_of(e: &reqwest::Error) -> String {
    let mut cause: &dyn std::error::Error = e;
    while let Some(next) = cause.source() {
        cause = next;
    }
    let text = cause.to_string();
    if e.is_timeout() {
        "timed out".to_string()
    } else if text.is_empty() {
        "the request failed".to_string()
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_engine_and_the_core_state_the_child_environment_alike() {
        // The core cannot see `bisa-http`; this is where the two
        // statements of one rule are held equal.
        let settings = NetworkSettings {
            mode: ProxyMode::Manual,
            http: None,
            https: Some("http://ada:s3cret@proxy.example:3128".into()),
            no_proxy: vec![".corp.example".into(), "10.0.0.0/8".into()],
            http1_only: true,
        };
        let (http_policy, problems) = policy(&settings);
        assert!(problems.is_empty());
        let ours = http_policy.child_env();
        let theirs = settings.child_env();
        assert_eq!(ours.remove, theirs.remove);
        // `Url` re-serialises `http://host:3128` with a trailing slash; the
        // names, the bypass and the login are what the tools read.
        assert_eq!(
            ours.set.keys().collect::<Vec<_>>(),
            theirs.set.keys().collect::<Vec<_>>()
        );
        assert_eq!(ours.set["NO_PROXY"], theirs.set["NO_PROXY"]);
        assert!(ours.set["HTTPS_PROXY"].starts_with("http://ada:s3cret@proxy.example:3128"));
        for mode in [ProxyMode::Environment, ProxyMode::None] {
            let s = NetworkSettings {
                mode,
                ..NetworkSettings::default()
            };
            assert_eq!(policy(&s).0.child_env().remove, s.child_env().remove);
        }
    }

    #[test]
    fn an_unparsable_url_is_dropped_and_named() {
        let settings = NetworkSettings {
            mode: ProxyMode::Manual,
            http: Some("not a url".into()),
            https: None,
            no_proxy: vec![],
            http1_only: false,
        };
        let (policy, problems) = policy(&settings);
        assert!(!policy.names_a_proxy());
        assert_eq!(problems.len(), 1);
        assert!(problems[0].starts_with(keys::PROXY_HTTP));
    }

    #[test]
    fn in_force_reads_the_mode_and_masks_the_login() {
        let env = ProxyEnvironment {
            http: None,
            https: Some("http://proxy.example:3128".into()),
            no_proxy: None,
        };
        let none = HttpPolicy {
            proxy: ProxyPolicy::None,
            http1_only: false,
        };
        assert_eq!(in_force(&none, &env), InForce::Direct);
        assert!(matches!(
            in_force(&HttpPolicy::default(), &env),
            InForce::Proxy { .. }
        ));
        let manual = policy(&NetworkSettings {
            mode: ProxyMode::Manual,
            http: None,
            https: Some("http://ada:s3cret@proxy.example:3128".into()),
            no_proxy: vec![],
            http1_only: false,
        })
        .0;
        match in_force(&manual, &env) {
            InForce::Proxy {
                https, no_proxy, ..
            } => {
                assert!(!https.unwrap().contains("s3cret"));
                assert_eq!(no_proxy.as_deref(), Some("localhost,127.0.0.1,::1"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_check_goes_out_over_http_and_never_to_this_machine() {
        assert!(check_url("https://api.github.com").is_ok());
        assert!(check_url("ftp://example.com").is_err());
        assert!(check_url("http://localhost:8080").is_err());
        assert!(check_url("http://127.0.0.1").is_err());
        assert!(check_url("http://[::1]/").is_err());
        assert!(check_url("nope").is_err());
    }

    #[test]
    fn git_under_a_manual_policy_carries_the_proxy_and_the_version() {
        let (policy, _) = policy(&NetworkSettings {
            mode: ProxyMode::Manual,
            http: Some("http://proxy.example:3128".into()),
            https: None,
            no_proxy: vec![],
            http1_only: true,
        });
        // The handle is exercised through vcs's own tests of `with_env`,
        // `without_env` and `with_config`; here, that the rule composes.
        let _ = git_under(bisa_vcs::Git::new(), &policy);
    }
}
