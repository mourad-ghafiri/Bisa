//! The `network.*` settings, read once into a typed struct — and the two
//! rules every reader of them shares.
//!
//! A proxy is one truth with four readers: the HTTP clients the platform
//! builds (`bisa-http`), the environment every child it runs inherits
//! (a harness session, a harness launched in a terminal, git, `gh`/`glab`, a
//! workstream script), the status route, and the Settings panel. The rules
//! are stated here, beside the registry that declares the keys, so the
//! defaults live in one place per key (the `def!` in `settings.rs`) and a
//! test holds this struct's fallbacks equal to them.
//!
//! Two things a proxy URL must never do on any surface but the field a
//! person edits: show its password (`masked`), and carry a port in the bypass
//! list where nothing would match it (`check_write`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::settings::{Resolved, SettingsError};

/// The `network.*` keys, spelled once.
pub mod keys {
    pub const PROXY_MODE: &str = "network.proxy.mode";
    pub const PROXY_HTTP: &str = "network.proxy.http";
    pub const PROXY_HTTPS: &str = "network.proxy.https";
    pub const PROXY_NO_PROXY: &str = "network.proxy.no_proxy";
    pub const HTTP1_ONLY: &str = "network.http1_only";
    /// The echo service the desktop asks for this machine's public address.
    /// Not a field of [`NetworkSettings`]: the clients never read it.
    pub const PUBLIC_IP_URL: &str = "network.public_ip_url";
}

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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProxyMode {
    /// The proxy the node's own environment names, if any.
    Environment,
    /// No proxy, whatever the environment says.
    None,
    /// The two URLs and the bypass list.
    Manual,
}

impl ProxyMode {
    /// The choice words, in the registry's order.
    pub const WORDS: [&str; 3] = ["environment", "none", "manual"];

    pub fn as_str(self) -> &'static str {
        match self {
            ProxyMode::Environment => "environment",
            ProxyMode::None => "none",
            ProxyMode::Manual => "manual",
        }
    }

    pub fn parse(word: &str) -> Option<Self> {
        match word {
            "environment" => Some(ProxyMode::Environment),
            "none" => Some(ProxyMode::None),
            "manual" => Some(ProxyMode::Manual),
            _ => None,
        }
    }
}

/// The resolved network configuration. Construct from resolved settings with
/// [`NetworkSettings::from_resolved`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkSettings {
    pub mode: ProxyMode,
    /// The proxy for `http://` targets, as written — a login included.
    pub http: Option<String>,
    /// The proxy for `https://` targets, as written.
    pub https: Option<String>,
    /// The bypass entries, trimmed, empty ones dropped.
    pub no_proxy: Vec<String>,
    pub http1_only: bool,
}

impl Default for NetworkSettings {
    /// The registry defaults, in one place; `from_resolved` overrides from the
    /// resolved layers. A test asserts these equal the `def!` defaults.
    fn default() -> Self {
        Self {
            mode: ProxyMode::Environment,
            http: None,
            https: None,
            no_proxy: Vec::new(),
            http1_only: false,
        }
    }
}

/// What a child the platform runs is handed: names set, then names removed.
/// Removal is what makes `none` mean none — a child would otherwise inherit
/// whatever the node's own environment says.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChildEnv {
    pub set: BTreeMap<String, String>,
    pub remove: Vec<&'static str>,
}

impl NetworkSettings {
    /// Read the `network.*` values out of a resolved settings list, falling
    /// back to [`Default`] for any key the list omits or gives a wrong-typed
    /// value. An empty URL is none; the bypass list is split on commas.
    pub fn from_resolved(resolved: &[Resolved]) -> Self {
        let d = Self::default();
        let find = |key: &str| resolved.iter().find(|r| r.key.as_str() == key);
        let text = |key: &str| {
            find(key)
                .and_then(|r| r.value.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        };
        Self {
            mode: find(keys::PROXY_MODE)
                .and_then(|r| r.value.as_str())
                .and_then(ProxyMode::parse)
                .unwrap_or(d.mode),
            http: text(keys::PROXY_HTTP),
            https: text(keys::PROXY_HTTPS),
            no_proxy: text(keys::PROXY_NO_PROXY)
                .map(|s| bypass_entries(&s))
                .unwrap_or(d.no_proxy),
            http1_only: find(keys::HTTP1_ONLY)
                .and_then(|r| r.value.as_bool())
                .unwrap_or(d.http1_only),
        }
    }

    /// Whether the manual proxy names anything at all.
    pub fn names_a_proxy(&self) -> bool {
        self.http.is_some() || self.https.is_some()
    }

    /// The bypass list as the tools read it: this machine's own names first,
    /// then the person's entries, comma-joined.
    pub fn no_proxy_value(&self) -> String {
        LOOPBACK_NAMES
            .iter()
            .map(|s| s.to_string())
            .chain(
                self.no_proxy
                    .iter()
                    .filter(|e| !LOOPBACK_NAMES.contains(&e.as_str()))
                    .cloned(),
            )
            .collect::<Vec<_>>()
            .join(",")
    }

    /// The environment every child the platform runs is given.
    ///
    /// `environment`: nothing set and nothing removed — the child inherits
    /// what the node inherited. `none`: every proxy name removed. `manual`:
    /// both cases of `HTTP_PROXY` and `HTTPS_PROXY` set where a URL is named
    /// and removed where none is, the bypass under both cases of `NO_PROXY`,
    /// and `ALL_PROXY` removed so nothing inherited outranks the manual pair.
    pub fn child_env(&self) -> ChildEnv {
        match self.mode {
            ProxyMode::Environment => ChildEnv::default(),
            ProxyMode::None => ChildEnv {
                set: BTreeMap::new(),
                remove: PROXY_ENV_NAMES.to_vec(),
            },
            ProxyMode::Manual => {
                let mut set = BTreeMap::new();
                let mut remove = vec!["ALL_PROXY", "all_proxy"];
                match &self.http {
                    Some(url) => {
                        set.insert("HTTP_PROXY".to_string(), url.clone());
                        set.insert("http_proxy".to_string(), url.clone());
                    }
                    None => remove.extend(["HTTP_PROXY", "http_proxy"]),
                }
                match &self.https {
                    Some(url) => {
                        set.insert("HTTPS_PROXY".to_string(), url.clone());
                        set.insert("https_proxy".to_string(), url.clone());
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
}

/// The bypass text as entries: split on commas, trimmed, empties dropped.
pub fn bypass_entries(text: &str) -> Vec<String> {
    text.split(',')
        .map(str::trim)
        .filter(|e| !e.is_empty())
        .map(str::to_string)
        .collect()
}

/// A proxy URL with its password replaced, the login kept — the only form a
/// proxy URL takes on any surface but the field a person edits. A URL that
/// does not parse is answered as written, since it can carry no password the
/// parser would find.
pub fn masked(url: &str) -> String {
    match url::Url::parse(url) {
        Ok(mut parsed) if parsed.password().is_some() => {
            // `set_password` percent-encodes anything outside the userinfo
            // set — the bullets would read `%E2%80%A2` — so the password is
            // dropped and the mask written by hand after the login.
            if parsed.set_password(None).is_err() {
                return url.to_string();
            }
            let shown = parsed.to_string();
            match shown.find('@') {
                Some(at) => format!("{}:\u{2022}\u{2022}\u{2022}{}", &shown[..at], &shown[at..]),
                None => shown,
            }
        }
        _ => url.to_string(),
    }
}

/// The rules a `network.*` value must meet beyond its kind, checked at write
/// so a refusal names the key and the rule like every other one.
pub fn check_write(key: &str, value: &Value) -> Result<(), SettingsError> {
    let bad = |why: String| SettingsError::InvalidValue {
        key: key.to_string(),
        why,
    };
    match key {
        keys::PROXY_HTTP | keys::PROXY_HTTPS => {
            let text = value.as_str().unwrap_or("").trim();
            if text.is_empty() {
                return Ok(());
            }
            check_proxy_url(text).map_err(bad)
        }
        keys::PROXY_NO_PROXY => {
            let text = value.as_str().unwrap_or("");
            for entry in bypass_entries(text) {
                check_bypass_entry(&entry).map_err(bad)?;
            }
            Ok(())
        }
        keys::PUBLIC_IP_URL => {
            let text = value.as_str().unwrap_or("").trim();
            if text.is_empty() {
                return Ok(());
            }
            check_echo_url(text).map_err(bad)
        }
        _ => Ok(()),
    }
}

/// An echo service is an `https://` URL with a host that is out on the
/// internet, not this machine; a path and a query are its own business
/// (`/ip`, `?format=text`).
fn check_echo_url(text: &str) -> Result<(), String> {
    let parsed = url::Url::parse(text)
        .map_err(|_| "expected a URL such as https://api.ipify.org".to_string())?;
    if parsed.scheme() != "https" {
        return Err(format!(
            "expected an https:// URL, not {}://",
            parsed.scheme()
        ));
    }
    let host = parsed
        .host_str()
        .filter(|h| !h.is_empty())
        .ok_or_else(|| "expected a host in the URL".to_string())?;
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    let this_machine = LOOPBACK_NAMES.contains(&bare)
        || bare
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback() || ip.is_unspecified());
    if this_machine {
        return Err("the echo service must be out on the internet, not this machine".to_string());
    }
    Ok(())
}

/// A proxy is `http://` or `https://`, a host, an optional port and an
/// optional login — nothing after the authority, since a proxy is a place
/// to connect to, not a page.
fn check_proxy_url(text: &str) -> Result<(), String> {
    let parsed = url::Url::parse(text)
        .map_err(|_| "expected a URL such as http://proxy.example:3128".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(format!(
            "expected an http:// or https:// proxy, not {}://",
            parsed.scheme()
        ));
    }
    if parsed.host_str().map(str::is_empty).unwrap_or(true) {
        return Err("expected a host in the proxy URL".to_string());
    }
    if parsed.path() != "/" && !parsed.path().is_empty() || parsed.query().is_some() {
        return Err("a proxy URL ends at its host and port — no path or query".to_string());
    }
    Ok(())
}

/// A bypass entry is a name, a `.suffix`, an address or a CIDR — never a
/// port, which the matcher would not compare, and never a scheme.
fn check_bypass_entry(entry: &str) -> Result<(), String> {
    if entry.chars().any(char::is_whitespace) {
        return Err(format!("`{entry}` has whitespace in it"));
    }
    if entry.contains("://") {
        return Err(format!("`{entry}` is a URL; a bypass entry is a host"));
    }
    let bare = entry
        .trim_start_matches('[')
        .split(']')
        .next()
        .unwrap_or(entry);
    // `::1` and other IPv6 addresses carry colons of their own; a port is a
    // colon after a name or an IPv4 address.
    if !entry.contains('[') && bare.matches(':').count() == 1 {
        return Err(format!("`{entry}` names a port; a bypass entry never does"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{resolve_all, Origin};
    use serde_json::json;

    fn resolved(pairs: &[(&str, Value)]) -> Vec<Resolved> {
        pairs
            .iter()
            .map(|(k, v)| Resolved {
                key: k.to_string(),
                value: v.clone(),
                origin: Origin::Machine,
            })
            .collect()
    }

    /// Resolving with no values set yields exactly the struct's own defaults —
    /// the one guard against the `def!` defaults and [`Default`] drifting apart.
    #[test]
    fn defaults_match_the_registry() {
        assert_eq!(
            NetworkSettings::from_resolved(&resolve_all(&[])),
            NetworkSettings::default()
        );
        assert_eq!(
            crate::settings::SettingDef::lookup(keys::PROXY_MODE)
                .map(|d| match &d.kind {
                    crate::settings::Kind::Choice(words) => words.to_vec(),
                    _ => vec![],
                })
                .unwrap_or_default(),
            ProxyMode::WORDS.to_vec()
        );
        assert_eq!(
            crate::settings::SettingDef::lookup(keys::PUBLIC_IP_URL)
                .map(|d| d.default.clone())
                .unwrap_or_default(),
            json!("https://api.ipify.org"),
            "the echo service's default is the registry's"
        );
    }

    #[test]
    fn an_echo_service_is_an_https_url_with_a_host_or_empty() {
        let ok = |v: &str| check_write(keys::PUBLIC_IP_URL, &json!(v)).is_ok();
        for good in [
            "",
            "  ",
            "https://api.ipify.org",
            "https://checkip.amazonaws.com/",
            "https://ifconfig.me/ip",
            "https://api64.ipify.org?format=text",
        ] {
            assert!(ok(good), "{good:?} is an echo service");
        }
        for bad in [
            "http://api.ipify.org",
            "api.ipify.org",
            "/ip",
            "https://localhost/ip",
            "https://127.0.0.1",
            "https://[::1]/ip",
            "https://0.0.0.0",
        ] {
            assert!(!ok(bad), "{bad:?} is not an echo service");
        }
        assert!(matches!(
            check_write(keys::PUBLIC_IP_URL, &json!("http://api.ipify.org")),
            Err(SettingsError::InvalidValue { key, .. }) if key == keys::PUBLIC_IP_URL
        ));
    }

    #[test]
    fn a_manual_proxy_is_read_with_its_bypass_split_and_trimmed() {
        let ns = NetworkSettings::from_resolved(&resolved(&[
            (keys::PROXY_MODE, json!("manual")),
            (keys::PROXY_HTTP, json!(" http://proxy.example:3128 ")),
            (keys::PROXY_HTTPS, json!("")),
            (
                keys::PROXY_NO_PROXY,
                json!("a.example, .corp.example,, 10.0.0.0/8 "),
            ),
            (keys::HTTP1_ONLY, json!(true)),
        ]));
        assert_eq!(ns.mode, ProxyMode::Manual);
        assert_eq!(ns.http.as_deref(), Some("http://proxy.example:3128"));
        assert_eq!(ns.https, None);
        assert_eq!(
            ns.no_proxy,
            vec!["a.example", ".corp.example", "10.0.0.0/8"]
        );
        assert!(ns.http1_only);
        assert_eq!(
            ns.no_proxy_value(),
            "localhost,127.0.0.1,::1,a.example,.corp.example,10.0.0.0/8"
        );
    }

    #[test]
    fn the_child_environment_follows_the_mode() {
        let mut ns = NetworkSettings::default();
        assert_eq!(ns.child_env(), ChildEnv::default());

        ns.mode = ProxyMode::None;
        let env = ns.child_env();
        assert!(env.set.is_empty());
        assert_eq!(env.remove, PROXY_ENV_NAMES.to_vec());

        ns.mode = ProxyMode::Manual;
        ns.https = Some("http://u:p@proxy.example:3128".into());
        ns.no_proxy = vec![".corp.example".into()];
        let env = ns.child_env();
        assert_eq!(
            env.set.get("HTTPS_PROXY").map(String::as_str),
            Some("http://u:p@proxy.example:3128")
        );
        assert_eq!(env.set.get("https_proxy"), env.set.get("HTTPS_PROXY"));
        assert_eq!(
            env.set.get("NO_PROXY").map(String::as_str),
            Some("localhost,127.0.0.1,::1,.corp.example")
        );
        assert!(!env.set.contains_key("HTTP_PROXY"));
        assert_eq!(
            env.remove,
            vec!["ALL_PROXY", "all_proxy", "HTTP_PROXY", "http_proxy"]
        );
    }

    #[test]
    fn a_password_is_masked_and_a_login_kept() {
        assert_eq!(
            masked("http://ada:s3cret@proxy.example:3128"),
            "http://ada:\u{2022}\u{2022}\u{2022}@proxy.example:3128/"
        );
        assert_eq!(
            masked("http://proxy.example:3128"),
            "http://proxy.example:3128"
        );
        assert_eq!(masked("not a url"), "not a url");
        assert!(!masked("http://ada:s3cret@proxy.example").contains("s3cret"));
        assert_eq!(
            masked("http://ada:p%40ss@proxy.example"),
            "http://ada:\u{2022}\u{2022}\u{2022}@proxy.example/",
            "an encoded password leaves no trace either"
        );
        assert_eq!(
            masked("http://ada@proxy.example"),
            "http://ada@proxy.example",
            "a login alone is not a secret"
        );
        let nameless = masked("http://:s3cret@proxy.example:3128");
        assert!(
            !nameless.contains("s3cret"),
            "a password with no login still leaves: {nameless}"
        );
        assert_eq!(masked(""), "");
    }

    #[test]
    fn a_proxy_url_and_a_bypass_entry_are_checked_at_write() {
        let ok = |key: &str, v: &str| check_write(key, &json!(v)).is_ok();
        assert!(ok(keys::PROXY_HTTP, ""));
        assert!(ok(keys::PROXY_HTTP, "http://proxy.example:3128"));
        assert!(ok(keys::PROXY_HTTPS, "https://u:p@proxy.example"));
        assert!(!ok(keys::PROXY_HTTP, "proxy.example:3128"));
        assert!(!ok(keys::PROXY_HTTP, "socks5://proxy.example:1080"));
        assert!(!ok(keys::PROXY_HTTP, "http://proxy.example/path"));
        assert!(!ok(keys::PROXY_HTTP, "http://proxy.example?x=1"));

        assert!(ok(keys::PROXY_NO_PROXY, ""));
        assert!(ok(
            keys::PROXY_NO_PROXY,
            "a.example, .corp.example, 10.0.0.0/8, ::1, [fe80::1]"
        ));
        assert!(!ok(keys::PROXY_NO_PROXY, "a.example:8080"));
        assert!(!ok(keys::PROXY_NO_PROXY, "a example"));
        assert!(!ok(keys::PROXY_NO_PROXY, "http://a.example"));
        assert!(matches!(
            check_write(keys::PROXY_NO_PROXY, &json!("a:1")),
            Err(SettingsError::InvalidValue { key, .. }) if key == keys::PROXY_NO_PROXY
        ));
        // Keys of other groups are none of this module's business.
        assert!(check_write("editor.tab_size", &json!(40)).is_ok());
    }
}
