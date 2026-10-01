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

/// A relay URL a person may write: `ws://` or `wss://`, a host, nothing
/// else. What `sync.relays` holds and the panel's field checks.
pub fn is_relay_url(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url.trim()) else {
        return false;
    };
    matches!(parsed.scheme(), "ws" | "wss") && parsed.host_str().is_some_and(|h| !h.is_empty())
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
    /// The relay URLs that passed [`is_relay_url`], deduplicated, in order.
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
                if is_relay_url(url) && !relays.iter().any(|r| r == url) {
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
}
