//! The `sync.*`, `collab.*` and `security.collaboration.*` settings, read once
//! into typed structs — the words the host's pump, the guest's session and the
//! engine's admission share (14-collaboration).
//!
//! Stated here, beside the registry that declares the keys, so each default
//! lives in one place (the `def!` in `settings.rs`) and a test holds these
//! structs' fallbacks equal to them, as `network_settings.rs` does.

use serde::{Deserialize, Serialize};

use crate::member::MemberRole;
use crate::settings::Resolved;

/// The keys, spelled once.
pub mod keys {
    pub const SYNC_ENABLED: &str = "sync.enabled";
    pub const SYNC_RELAYS: &str = "sync.relays";
    pub const SYNC_INTERVAL_SECS: &str = "sync.interval_secs";
    pub const SYNC_PUBLISH_RELAY_LIST: &str = "sync.publish_relay_list";
    pub const SYNC_IROH_ENABLED: &str = "sync.iroh.enabled";
    pub const SYNC_IROH_N0_RELAYS: &str = "sync.iroh.n0_relays";
    pub const SYNC_IROH_PEERS: &str = "sync.iroh.peers";
    pub const COLLAB_NAME: &str = "collab.name";
    pub const COLLAB_JOIN: &str = "collab.join";
    pub const COLLAB_DEFAULT_ROLE: &str = "collab.default_role";
    pub const COLLAB_INVITE_TTL_HOURS: &str = "collab.invite_ttl_hours";
    pub const SECURITY_COLLAB_CLASSIFY: &str = "security.collaboration.classify";
    pub const SECURITY_COLLAB_AGENT_TOOLS: &str = "security.collaboration.agent_tools";
}

/// The relays a fresh node is given — four public Nostr relays, in this
/// order. What `sync.relays` holds until a person changes it; the wire is
/// off until `sync.enabled` is turned on, so nothing is contacted by default.
pub const DEFAULT_RELAYS: [&str; 4] = [
    "wss://relay.nostr.com",
    "wss://relay.nostr.net",
    "wss://relay.damus.io",
    "wss://nos.lol",
];

/// Rules a `sync.*` value has beyond its kind, checked when it is written:
/// every entry of `sync.relays` is a relay URL, so a value the pool would
/// only drop in silence is refused with the entry named.
pub fn check_write(key: &str, value: &serde_json::Value) -> Result<(), crate::SettingsError> {
    if key != keys::SYNC_RELAYS {
        return Ok(());
    }
    let bad = |why: String| crate::SettingsError::InvalidValue {
        key: key.to_string(),
        why,
    };
    let Some(entries) = value.as_array() else {
        return Err(bad("expected a list of relay URLs".into()));
    };
    for entry in entries {
        let Some(url) = entry.as_str() else {
            return Err(bad(format!("{entry} is not a relay URL")));
        };
        if !is_relay_url(url) {
            return Err(bad(format!(
                "{url:?} is not a relay URL: `wss://host` (or `ws://` on this machine)"
            )));
        }
    }
    Ok(())
}

/// A relay URL a person may write: `wss://` with a host, anywhere — or
/// `ws://` on this machine alone (`localhost`, a loopback or unspecified
/// address, bracketed or not), since a plaintext relay elsewhere would carry
/// this node's metadata in the clear. What `sync.relays` holds, the panel's
/// field checks and an invite's relays are held to.
pub fn is_relay_url(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url.trim()) else {
        return false;
    };
    match parsed.scheme() {
        "wss" => parsed.host_str().is_some_and(|h| !h.is_empty()),
        "ws" => match parsed.host() {
            Some(url::Host::Domain(d)) => d.trim_end_matches('.').eq_ignore_ascii_case("localhost"),
            Some(url::Host::Ipv4(a)) => a.is_loopback() || a.is_unspecified(),
            Some(url::Host::Ipv6(a)) => {
                a.is_loopback()
                    || a.is_unspecified()
                    || a.to_ipv4_mapped()
                        .is_some_and(|v| v.is_loopback() || v.is_unspecified())
            }
            None => false, // LCOV_EXCL_LINE a special scheme's URL always parses with a host
        },
        _ => false,
    }
}

/// A relay's address in the one form two spellings of it share: the scheme
/// and the host in lowercase, a default port and a lone trailing slash left
/// out, the path and the query as written. What "the same relay" is decided
/// by — in the pool, in the list a setting holds, in a screen's *already
/// configured* — while the spelling a person typed is what is stored and
/// shown. `None` for what is no relay URL ([`is_relay_url`]).
///
/// Compared by spelling, `wss://Relay.Example.com` connected and read
/// *never tried* — the relay layer keeps a host in lowercase — and
/// `wss://a.example` beside `wss://a.example/` was two rows over one
/// connection, where removing one took the relay from under the other.
pub fn canonical_relay_url(url: &str) -> Option<String> {
    let url = url.trim();
    if !is_relay_url(url) {
        return None;
    }
    let parsed = url::Url::parse(url).ok()?;
    let mut out = format!("{}://{}", parsed.scheme(), parsed.host_str()?);
    if let Some(port) = parsed.port() {
        out.push_str(&format!(":{port}"));
    }
    out.push_str(parsed.path().trim_end_matches('/'));
    if let Some(query) = parsed.query() {
        out.push_str(&format!("?{query}"));
    }
    Some(out)
}

/// Whether two addresses are one relay ([`canonical_relay_url`]); two
/// strings that are no relay URL are the same only letter for letter.
pub fn same_relay(a: &str, b: &str) -> bool {
    match (canonical_relay_url(a), canonical_relay_url(b)) {
        (Some(a), Some(b)) => a == b,
        _ => a.trim() == b.trim(),
    }
}

/// A direct peer entered by hand: which member, their endpoint, where.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ManualPeer {
    /// The member's pubkey (64 lowercase hex chars).
    pub member_pubkey: String,
    /// The peer's endpoint id, in its display form.
    pub node_id: String,
    /// Direct socket addresses, `host:port`.
    #[serde(default)]
    pub addrs: Vec<String>,
}

/// The `sync.*` values: how this node reaches other nodes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncSettings {
    pub enabled: bool,
    /// The relay URLs that passed [`is_relay_url`], one per relay
    /// ([`same_relay`]) in the spelling it was first given, in order.
    pub relays: Vec<String>,
    pub interval_secs: u64,
    pub publish_relay_list: bool,
    pub iroh_enabled: bool,
    pub iroh_n0_relays: bool,
    pub iroh_peers: Vec<ManualPeer>,
}

impl Default for SyncSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            relays: DEFAULT_RELAYS.iter().map(|r| r.to_string()).collect(),
            interval_secs: 30,
            publish_relay_list: true,
            iroh_enabled: true,
            iroh_n0_relays: false,
            iroh_peers: Vec::new(),
        }
    }
}

/// How a valid invite is admitted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum JoinPolicy {
    /// A claimed code admits at once.
    Admit,
    /// A claimed code waits in the owner's Inbox.
    Ask,
}

impl JoinPolicy {
    pub fn parse(word: &str) -> Option<Self> {
        match word {
            "admit" => Some(JoinPolicy::Admit),
            "ask" => Some(JoinPolicy::Ask),
            _ => None,
        }
    }
}

/// The `collab.*` values: the room's word to the people it hosts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CollabSettings {
    /// Trimmed; empty when unset.
    pub name: String,
    pub join: JoinPolicy,
    pub default_role: MemberRole,
    pub invite_ttl_hours: u64,
}

impl Default for CollabSettings {
    fn default() -> Self {
        Self {
            name: String::new(),
            join: JoinPolicy::Admit,
            default_role: MemberRole::Guest,
            invite_ttl_hours: crate::invite::INVITE_TTL_HOURS_DEFAULT,
        }
    }
}

/// What an agent woken by an outsider may do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OutsiderTools {
    /// Every tool beyond reading is put to the owner.
    Ask,
    /// The same rules as for the owner's own messages.
    AsOwner,
}

impl OutsiderTools {
    pub fn parse(word: &str) -> Option<Self> {
        match word {
            "ask" => Some(OutsiderTools::Ask),
            "as_owner" => Some(OutsiderTools::AsOwner),
            _ => None,
        }
    }
}

/// The `security.collaboration.*` values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CollaborationSecurity {
    pub classify: bool,
    pub agent_tools: OutsiderTools,
}

impl Default for CollaborationSecurity {
    fn default() -> Self {
        Self {
            classify: true,
            agent_tools: OutsiderTools::Ask,
        }
    }
}

fn find<'a>(resolved: &'a [Resolved], key: &str) -> Option<&'a Resolved> {
    resolved.iter().find(|r| r.key.as_str() == key)
}

fn bool_of(resolved: &[Resolved], key: &str, fallback: bool) -> bool {
    find(resolved, key)
        .and_then(|r| r.value.as_bool())
        .unwrap_or(fallback)
}

fn u64_of(resolved: &[Resolved], key: &str, fallback: u64) -> u64 {
    find(resolved, key)
        .and_then(|r| r.value.as_u64())
        .unwrap_or(fallback)
}

fn word_of<'a>(resolved: &'a [Resolved], key: &str) -> Option<&'a str> {
    find(resolved, key).and_then(|r| r.value.as_str())
}

impl SyncSettings {
    /// Read the `sync.*` values out of a resolved settings list, falling back
    /// to [`Default`] for any key the list omits or gives a wrong-typed value.
    /// A relay that is not a `ws(s)://` URL is dropped, never carried.
    pub fn from_resolved(resolved: &[Resolved]) -> Self {
        let d = Self::default();
        let mut relays: Vec<String> = Vec::new();
        if let Some(list) = find(resolved, keys::SYNC_RELAYS).and_then(|r| r.value.as_array()) {
            for v in list {
                let Some(url) = v.as_str().map(str::trim) else {
                    continue;
                };
                if is_relay_url(url) && !relays.iter().any(|r| same_relay(r, url)) {
                    relays.push(url.to_string());
                }
            }
        }
        let iroh_peers = find(resolved, keys::SYNC_IROH_PEERS)
            .and_then(|r| serde_json::from_value::<Vec<ManualPeer>>(r.value.clone()).ok())
            .unwrap_or_default();
        Self {
            enabled: bool_of(resolved, keys::SYNC_ENABLED, d.enabled),
            relays,
            interval_secs: u64_of(resolved, keys::SYNC_INTERVAL_SECS, d.interval_secs).max(5),
            publish_relay_list: bool_of(
                resolved,
                keys::SYNC_PUBLISH_RELAY_LIST,
                d.publish_relay_list,
            ),
            iroh_enabled: bool_of(resolved, keys::SYNC_IROH_ENABLED, d.iroh_enabled),
            iroh_n0_relays: bool_of(resolved, keys::SYNC_IROH_N0_RELAYS, d.iroh_n0_relays),
            iroh_peers,
        }
    }

    /// Whether this node talks to anyone at all: on, and a relay to talk
    /// through or a direct endpoint.
    pub fn reaches_anyone(&self) -> bool {
        self.enabled && (!self.relays.is_empty() || self.iroh_enabled)
    }
}

impl CollabSettings {
    pub fn from_resolved(resolved: &[Resolved]) -> Self {
        let d = Self::default();
        Self {
            name: word_of(resolved, keys::COLLAB_NAME)
                .map(str::trim)
                .unwrap_or_default()
                .to_string(),
            join: word_of(resolved, keys::COLLAB_JOIN)
                .and_then(JoinPolicy::parse)
                .unwrap_or(d.join),
            default_role: word_of(resolved, keys::COLLAB_DEFAULT_ROLE)
                .and_then(|w| w.parse::<MemberRole>().ok())
                .filter(|r| r.is_hosted())
                .unwrap_or(d.default_role),
            invite_ttl_hours: u64_of(resolved, keys::COLLAB_INVITE_TTL_HOURS, d.invite_ttl_hours)
                .clamp(
                    crate::invite::INVITE_TTL_HOURS_MIN,
                    crate::invite::INVITE_TTL_HOURS_MAX,
                ),
        }
    }

    /// What a guest of this workspace sees it called: the name, else the
    /// owner's pubkey's first letters.
    pub fn display_name(&self, owner_hex: &str) -> String {
        if self.name.is_empty() {
            format!("{}…", owner_hex.chars().take(8).collect::<String>())
        } else {
            self.name.clone()
        }
    }
}

impl CollaborationSecurity {
    pub fn from_resolved(resolved: &[Resolved]) -> Self {
        let d = Self::default();
        Self {
            classify: bool_of(resolved, keys::SECURITY_COLLAB_CLASSIFY, d.classify),
            agent_tools: word_of(resolved, keys::SECURITY_COLLAB_AGENT_TOOLS)
                .and_then(OutsiderTools::parse)
                .unwrap_or(d.agent_tools),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{resolve_all, Origin};
    use serde_json::{json, Value};

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

    /// Resolving with no values set yields exactly the structs' own defaults.
    #[test]
    fn defaults_match_the_registry() {
        let all = resolve_all(&[]);
        assert_eq!(SyncSettings::from_resolved(&all), SyncSettings::default());
        assert_eq!(
            CollabSettings::from_resolved(&all),
            CollabSettings::default()
        );
        assert_eq!(
            CollaborationSecurity::from_resolved(&all),
            CollaborationSecurity::default()
        );
        for key in [
            keys::SYNC_ENABLED,
            keys::SYNC_RELAYS,
            keys::SYNC_INTERVAL_SECS,
            keys::SYNC_PUBLISH_RELAY_LIST,
            keys::SYNC_IROH_ENABLED,
            keys::SYNC_IROH_N0_RELAYS,
            keys::SYNC_IROH_PEERS,
            keys::COLLAB_NAME,
            keys::COLLAB_JOIN,
            keys::COLLAB_DEFAULT_ROLE,
            keys::COLLAB_INVITE_TTL_HOURS,
            keys::SECURITY_COLLAB_CLASSIFY,
            keys::SECURITY_COLLAB_AGENT_TOOLS,
        ] {
            assert!(
                crate::settings::SettingDef::lookup(key).is_some(),
                "{key} is not in the registry"
            );
        }
    }

    /// The four defaults are relay URLs, distinct, and reach nobody until
    /// the switch is on.
    #[test]
    fn the_default_relays_are_valid_distinct_and_quiet_until_turned_on() {
        let d = SyncSettings::default();
        assert_eq!(d.relays.len(), 4);
        for r in &d.relays {
            assert!(is_relay_url(r), "{r}");
            assert!(r.starts_with("wss://"), "{r}");
        }
        let distinct: std::collections::BTreeSet<&String> = d.relays.iter().collect();
        assert_eq!(distinct.len(), d.relays.len());
        assert!(!d.enabled, "off until a person turns it on");
        assert!(!d.reaches_anyone());
        let on = SyncSettings {
            enabled: true,
            ..SyncSettings::default()
        };
        assert!(on.reaches_anyone());
    }

    #[test]
    fn relays_are_ws_urls_deduplicated_and_the_interval_is_bounded() {
        assert!(is_relay_url("wss://relay.example"));
        assert!(is_relay_url("ws://127.0.0.1:7777"));
        // Plaintext on this machine alone: a name, a loopback address.
        assert!(is_relay_url("ws://localhost:7777"));
        assert!(is_relay_url("ws://LOCALHOST.:7777"));
        assert!(is_relay_url("ws://[::1]:7777"));
        assert!(
            is_relay_url("ws://[::]:7777"),
            "unspecified is this machine"
        );
        assert!(
            is_relay_url("ws://[::ffff:127.0.0.1]:7777"),
            "a mapped loopback"
        );
        assert!(
            is_relay_url("ws://[::ffff:0.0.0.0]:7777"),
            "a mapped unspecified"
        );
        assert!(
            !is_relay_url("ws://[::ffff:10.0.0.5]:7777"),
            "a mapped LAN address is not this machine"
        );
        assert!(
            !is_relay_url("ws://[fe80::1]:7777"),
            "link-local is not loopback"
        );
        assert!(is_relay_url("ws://0.0.0.0:7777"));
        assert!(!is_relay_url("ws://relay.example"));
        assert!(!is_relay_url("ws://10.0.0.5:7777"));
        assert!(!is_relay_url("ws://localhost.example:7777"));
        assert!(!is_relay_url("https://relay.example"));
        assert!(!is_relay_url("wss://"));
        assert!(!is_relay_url("relay.example"));
        let s = SyncSettings::from_resolved(&resolved(&[
            (
                keys::SYNC_RELAYS,
                json!([
                    "wss://a.example",
                    " wss://a.example ",
                    "https://no",
                    7,
                    "wss://b.example"
                ]),
            ),
            (keys::SYNC_INTERVAL_SECS, json!(1)),
            (keys::SYNC_ENABLED, json!(false)),
            (
                keys::SYNC_IROH_PEERS,
                json!([{"member_pubkey": "ab", "node_id": "n1", "addrs": ["10.0.0.2:1"]}]),
            ),
        ]));
        assert_eq!(s.relays, vec!["wss://a.example", "wss://b.example"]);
        assert_eq!(s.interval_secs, 5, "never under the floor");
        assert!(!s.enabled);
        assert!(!s.reaches_anyone());
        assert_eq!(s.iroh_peers[0].node_id, "n1");
        let alone = SyncSettings {
            enabled: true,
            relays: vec![],
            iroh_enabled: false,
            ..Default::default()
        };
        assert!(!alone.reaches_anyone());
    }

    #[test]
    fn the_room_s_words_fall_to_their_defaults_and_never_name_the_owner_role() {
        let c = CollabSettings::from_resolved(&resolved(&[
            (keys::COLLAB_NAME, json!("  Acme  ")),
            (keys::COLLAB_JOIN, json!("ask")),
            (keys::COLLAB_DEFAULT_ROLE, json!("owner")),
            (keys::COLLAB_INVITE_TTL_HOURS, json!(9999)),
        ]));
        assert_eq!(c.name, "Acme");
        assert_eq!(c.join, JoinPolicy::Ask);
        assert_eq!(
            c.default_role,
            MemberRole::Guest,
            "an invite never names the owner"
        );
        assert_eq!(c.invite_ttl_hours, crate::invite::INVITE_TTL_HOURS_MAX);
        assert_eq!(c.display_name("abcdef0123456789"), "Acme");
        assert_eq!(
            CollabSettings::default().display_name("abcdef0123456789"),
            "abcdef01…"
        );
        let member = CollabSettings::from_resolved(&resolved(&[(
            keys::COLLAB_DEFAULT_ROLE,
            json!("member"),
        )]));
        assert_eq!(member.default_role, MemberRole::Member);
        let sec = CollaborationSecurity::from_resolved(&resolved(&[
            (keys::SECURITY_COLLAB_CLASSIFY, json!(false)),
            (keys::SECURITY_COLLAB_AGENT_TOOLS, json!("as_owner")),
        ]));
        assert!(!sec.classify);
        assert_eq!(sec.agent_tools, OutsiderTools::AsOwner);
    }

    /// A relay entry the pool would only drop in silence is refused when it
    /// is written, with the entry named; a list of relay URLs passes.
    #[test]
    fn a_relay_that_is_no_url_is_refused_when_written_not_dropped_when_read() {
        let ok = serde_json::json!(["wss://relay.example", "ws://127.0.0.1:7777"]);
        assert!(check_write(keys::SYNC_RELAYS, &ok).is_ok());
        for bad in [
            serde_json::json!(["not-a-url"]),
            serde_json::json!(["https://relay.example"]),
            serde_json::json!(["wss://"]),
            serde_json::json!([7]),
            serde_json::json!("wss://relay.example"),
        ] {
            let refused = check_write(keys::SYNC_RELAYS, &bad).unwrap_err();
            assert!(
                refused.to_string().contains("sync.relays"),
                "{bad}: {refused}"
            );
        }
        assert!(
            refused_names_the_entry(),
            "the refusal names the entry a person wrote"
        );
        // Another key is not this check's business.
        assert!(check_write("sync.enabled", &serde_json::json!("junk")).is_ok());
    }

    fn refused_names_the_entry() -> bool {
        check_write(keys::SYNC_RELAYS, &serde_json::json!(["not-a-url"]))
            .unwrap_err()
            .to_string()
            .contains("not-a-url")
    }

    // added by the coverage pass: collab_settings.rs

    #[test]
    fn a_policy_word_nobody_knows_is_none_and_a_relay_entry_that_is_no_text_is_passed_over() {
        assert_eq!(JoinPolicy::parse("maybe"), None);
        assert_eq!(OutsiderTools::parse("maybe"), None);
        let sync = SyncSettings::from_resolved(&resolved(&[(
            keys::SYNC_RELAYS,
            json!([7, "wss://relay.example", "wss://relay.example", "http://no"]),
        )]));
        assert_eq!(sync.relays, vec!["wss://relay.example".to_string()]);
    }

    // added by the coverage pass: b5-collab_settings.rs
    #[test]
    fn a_relay_list_that_is_not_a_list_carries_nothing() {
        let settings = SyncSettings::from_resolved(&resolved(&[(
            keys::SYNC_RELAYS,
            json!("wss://one.example"),
        )]));
        assert!(settings.relays.is_empty());
    }

    /// Two spellings of one relay are one relay: the host's case, a default
    /// port, a lone trailing slash. A path, a port of its own and the scheme
    /// are the relay's; what is no relay URL has no canonical form.
    #[test]
    fn two_spellings_of_one_relay_are_one_relay() {
        let one = canonical_relay_url("wss://relay.example").unwrap();
        for spelling in [
            "wss://Relay.Example",
            "WSS://relay.example/",
            "wss://relay.example:443",
            "  wss://relay.example  ",
        ] {
            assert_eq!(
                canonical_relay_url(spelling).as_deref(),
                Some(&*one),
                "{spelling}"
            );
            assert!(same_relay(spelling, "wss://relay.example"), "{spelling}");
        }
        for another in [
            "wss://relay.example:7777",
            "wss://relay.example/nostr",
            "wss://other.example",
            "ws://127.0.0.1:7777",
        ] {
            assert!(!same_relay(another, "wss://relay.example"), "{another}");
        }
        assert_eq!(
            canonical_relay_url("wss://relay.example/nostr/?k=1").as_deref(),
            Some("wss://relay.example/nostr?k=1")
        );
        assert_eq!(canonical_relay_url("https://relay.example"), None);
        assert_eq!(
            canonical_relay_url("ws://relay.example"),
            None,
            "plaintext elsewhere"
        );
        assert!(same_relay("not a url", " not a url "));
        assert!(!same_relay("not a url", "wss://relay.example"));

        // The list a setting holds: one row a relay, in its first spelling.
        let resolved = [Resolved {
            key: keys::SYNC_RELAYS.into(),
            value: serde_json::json!([
                "wss://Relay.Example",
                "wss://relay.example/",
                "wss://other.example"
            ]),
            origin: crate::settings::Origin::Machine,
        }];
        assert_eq!(
            SyncSettings::from_resolved(&resolved).relays,
            ["wss://Relay.Example", "wss://other.example"]
        );
    }
}
