//! What the host pump runs with: the `sync.*` settings, read once into a
//! value. There is no file of its own — relays, the interval and the direct
//! transport are settings in the registry (`sync.relays`, `sync.interval_secs`,
//! `sync.iroh.*`), edited where every other setting is and heard live.

use crate::NetError;
use bisa_core::{ManualPeer, SyncSettings};
use bisa_store::Workspace;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetConfig {
    /// Whether the pump runs at all.
    pub enabled: bool,
    /// Relay websocket URLs, already checked to be `ws(s)://`.
    pub relays: Vec<String>,
    /// Catch-up interval in seconds.
    pub sync_interval_secs: u64,
    /// Say which relays this key reads (NIP-65) on start and on change.
    pub publish_relay_list: bool,
    /// Direct QUIC transport (takes effect when the `iroh` feature is
    /// compiled in).
    pub iroh: bool,
    /// Let iroh use n0's public relay servers for NAT traversal. Off by
    /// default: on, it contacts infrastructure operated by n0.
    pub iroh_n0_relays: bool,
    /// Peers entered by hand (pure-LAN, no-relay bootstrap).
    pub iroh_peers: Vec<ManualPeer>,
}

impl Default for NetConfig {
    fn default() -> Self {
        Self::from_settings(&SyncSettings::default())
    }
}

impl NetConfig {
    pub fn from_settings(s: &SyncSettings) -> Self {
        Self {
            enabled: s.enabled,
            relays: s.relays.clone(),
            sync_interval_secs: s.interval_secs.max(1),
            publish_relay_list: s.publish_relay_list,
            iroh: s.iroh_enabled,
            iroh_n0_relays: s.iroh_n0_relays,
            iroh_peers: s.iroh_peers.clone(),
        }
    }

    /// The workspace's resolved `sync.*` settings.
    pub fn from_workspace(ws: &Workspace) -> Result<Self, NetError> {
        let resolved = ws.settings(None)?;
        Ok(Self::from_settings(&SyncSettings::from_resolved(&resolved)))
    }

    /// Whether the pump has anyone to talk through: a relay, or the direct
    /// transport compiled in and on.
    pub fn reaches_anyone(&self) -> bool {
        self.enabled && (!self.relays.is_empty() || (cfg!(feature = "iroh") && self.iroh))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_the_registry_s_and_a_relay_makes_it_reach() {
        let cfg = NetConfig::default();
        assert!(!cfg.enabled, "off until a person turns it on");
        assert_eq!(cfg.relays.len(), 4, "the four default relays");
        assert!(
            !cfg.reaches_anyone(),
            "four relays and off still reach nobody"
        );
        assert_eq!(cfg.sync_interval_secs, 30);
        assert!(cfg.publish_relay_list);
        assert!(cfg.iroh);
        assert!(!cfg.iroh_n0_relays);
        let with_relay = NetConfig::from_settings(&SyncSettings {
            enabled: true,
            relays: vec!["wss://relay.example".into()],
            iroh_enabled: false,
            ..SyncSettings::default()
        });
        assert!(with_relay.reaches_anyone());
        let off = NetConfig::from_settings(&SyncSettings {
            enabled: false,
            relays: vec!["wss://relay.example".into()],
            ..SyncSettings::default()
        });
        assert!(!off.reaches_anyone());
        let floor = NetConfig::from_settings(&SyncSettings {
            interval_secs: 0,
            ..SyncSettings::default()
        });
        assert_eq!(floor.sync_interval_secs, 1);
    }
}
