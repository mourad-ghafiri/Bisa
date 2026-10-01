//! The `events.*` settings, read once into a typed struct.
//!
//! The engine holds one of these on its listening state, refreshed when an
//! `events.*` key changes, and the runtime reads every knob from it: the
//! ticker's period, a check start's timeout, a files scan's bounds, whether
//! public hooks are allowed, how often a pull request's state is asked of its
//! code host, the loop guard's depth, the rate a start may begin runs at and
//! how many occurrences may wait per listener. Keeping the extraction here —
//! beside the registry that declares the keys — means the defaults live in
//! exactly one place per key (the `def!` in `settings.rs`), and a test asserts
//! this struct's fallbacks agree with them, the way `cache_settings.rs` does
//! for the caches.
//!
//! `enabled` is the person's switch for this machine's listeners: off, the
//! ticker fires nothing, the queue's worker claims nothing and the hooks turn
//! callers away, and the listening records and their queue stay exactly where
//! they are until it is on again. It never stops a run's own waits and
//! boundary events — those belong to runs already going. It is not
//! [`EngineConfig::events_enabled`](https://docs.rs/bisa-engine), which says
//! whether the runtime is spawned at all — the tests' and the one-shot CLI's
//! switch, never a person's.

use std::time::Duration;

use crate::settings::Resolved;

/// How far a files scan of a plain folder may go: levels deep and entries
/// examined, whichever comes first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanBounds {
    pub depth: usize,
    pub entries: usize,
}

/// The resolved listening runtime configuration. Construct from resolved
/// settings with [`EventSettings::from_resolved`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventSettings {
    pub enabled: bool,
    pub tick_secs: u64,
    pub check_timeout_secs: u64,
    pub files_max_depth: u64,
    pub files_max_entries: u64,
    pub public_hooks: bool,
    pub pr_poll_secs: u64,
    pub chain_depth: u64,
    pub fires_per_minute: u64,
    pub backlog_per_listener: u64,
}

impl Default for EventSettings {
    /// The registry defaults, in one place; `from_resolved` overrides from the
    /// resolved layers. A test asserts these equal the `def!` defaults.
    fn default() -> Self {
        Self {
            enabled: true,
            tick_secs: 15,
            check_timeout_secs: 60,
            files_max_depth: 8,
            files_max_entries: 5_000,
            public_hooks: false,
            pr_poll_secs: 300,
            chain_depth: 3,
            fires_per_minute: 30,
            backlog_per_listener: 5,
        }
    }
}

/// The least a pull request's state may be asked of its code host at: a
/// code host rate-limits, and a minute is already brisk for a review.
pub const MIN_PR_POLL_SECS: u64 = 60;

impl EventSettings {
    /// Read the `events.*` values out of a resolved settings list, falling
    /// back to [`Default`] for any key the list omits or gives a wrong-typed
    /// value.
    pub fn from_resolved(resolved: &[Resolved]) -> Self {
        let d = Self::default();
        let u64_of = |key: &str, fallback: u64| {
            resolved
                .iter()
                .find(|r| r.key.as_str() == key)
                .and_then(|r| r.value.as_u64())
                .unwrap_or(fallback)
        };
        let bool_of = |key: &str, fallback: bool| {
            resolved
                .iter()
                .find(|r| r.key.as_str() == key)
                .and_then(|r| r.value.as_bool())
                .unwrap_or(fallback)
        };
        Self {
            enabled: bool_of("events.enabled", d.enabled),
            tick_secs: u64_of("events.tick_secs", d.tick_secs),
            check_timeout_secs: u64_of("events.check_timeout_secs", d.check_timeout_secs),
            files_max_depth: u64_of("events.files.max_depth", d.files_max_depth),
            files_max_entries: u64_of("events.files.max_entries", d.files_max_entries),
            public_hooks: bool_of("events.public_hooks", d.public_hooks),
            pr_poll_secs: u64_of("events.pr_poll_secs", d.pr_poll_secs),
            chain_depth: u64_of("events.chain_depth", d.chain_depth),
            fires_per_minute: u64_of("events.fires_per_minute", d.fires_per_minute),
            backlog_per_listener: u64_of("events.backlog_per_listener", d.backlog_per_listener),
        }
    }

    /// The ticker's period; never less than a second, whatever was stored.
    pub fn tick(&self) -> Duration {
        Duration::from_secs(self.tick_secs.max(1))
    }

    /// How long a check start's command may run before it counts as failing.
    pub fn check_timeout(&self) -> Duration {
        Duration::from_secs(self.check_timeout_secs.max(1))
    }

    /// How far a files scan may go.
    pub fn scan_bounds(&self) -> ScanBounds {
        ScanBounds {
            depth: self.files_max_depth.max(1) as usize,
            entries: self.files_max_entries.max(1) as usize,
        }
    }

    /// How often a watched pull request's state is asked of its code host;
    /// never more often than [`MIN_PR_POLL_SECS`].
    pub fn pr_poll(&self) -> Duration {
        Duration::from_secs(self.pr_poll_secs.max(MIN_PR_POLL_SECS))
    }

    /// The loop guard's depth, as the chain counts it; at least one hop.
    pub fn chain_cap(&self) -> u32 {
        u32::try_from(self.chain_depth.max(1)).unwrap_or(u32::MAX)
    }

    /// How many occurrences may wait per listener; at least one.
    pub fn backlog(&self) -> usize {
        usize::try_from(self.backlog_per_listener.max(1)).unwrap_or(usize::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::resolve_all;

    /// Resolving with no values set yields exactly the struct's own defaults —
    /// the one guard against the `def!` defaults and [`Default`] drifting apart.
    #[test]
    fn defaults_match_the_registry() {
        let resolved = resolve_all(&[]);
        assert_eq!(
            EventSettings::from_resolved(&resolved),
            EventSettings::default()
        );
    }

    #[test]
    fn public_hooks_start_off() {
        assert!(!EventSettings::default().public_hooks);
    }

    #[test]
    fn the_durations_and_bounds_follow_the_fields_and_never_reach_zero() {
        let s = EventSettings {
            tick_secs: 0,
            check_timeout_secs: 7,
            files_max_depth: 0,
            files_max_entries: 12,
            pr_poll_secs: 5,
            chain_depth: 0,
            backlog_per_listener: 0,
            ..Default::default()
        };
        assert_eq!(s.tick(), Duration::from_secs(1), "a zero tick would spin");
        assert_eq!(s.check_timeout(), Duration::from_secs(7));
        assert_eq!(
            s.scan_bounds(),
            ScanBounds {
                depth: 1,
                entries: 12
            }
        );
        assert_eq!(s.pr_poll(), Duration::from_secs(MIN_PR_POLL_SECS));
        assert_eq!(s.chain_cap(), 1);
        assert_eq!(s.backlog(), 1);
    }
}
