//! The `cache.*` settings, read once into a typed struct.
//!
//! The engine holds one of these, refreshed when settings change, and every
//! cache reads its TTL from it. Keeping the extraction here — beside the
//! registry that declares the keys — means the defaults live in exactly one
//! place per key (the `def!` in `settings.rs`), and a test asserts this struct's
//! fallbacks agree with them.
//!
//! `enabled` is the master switch: when it is off, every TTL getter returns
//! `Duration::ZERO`, which the cache toolkit treats as "always miss" — the whole
//! cache layer turns transparent without any code path being removed.

use std::time::Duration;

use crate::settings::Resolved;

/// The resolved cache configuration. Construct from resolved settings with
/// [`CacheSettings::from_resolved`]; read TTLs through the getters, which honor
/// [`enabled`](Self::enabled).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheSettings {
    pub enabled: bool,
    pub harness_listing_ttl_ms: u64,
    pub harness_models_ttl_ms: u64,
    pub git_status_ttl_ms: u64,
    pub path_index_ttl_ms: u64,
    pub path_index_max_roots: u32,
    pub codehost_ttl_ms: u64,
    pub codehost_max_entries: u32,
    pub presence_ttl_ms: u64,
    pub model_health_ttl_ms: u64,
    pub harness_usage_ttl_ms: u64,
    pub desktop_ports_poll_ms: u64,
    pub desktop_stats_poll_ms: u64,
    pub desktop_disk_poll_ms: u64,
    pub desktop_network_poll_ms: u64,
    pub desktop_sessions_safety_ms: u64,
}

impl Default for CacheSettings {
    /// The registry defaults, in one place; `from_resolved` overrides from the
    /// resolved layers. A test asserts these equal the `def!` defaults.
    fn default() -> Self {
        Self {
            enabled: true,
            harness_listing_ttl_ms: 30_000,
            harness_models_ttl_ms: 300_000,
            git_status_ttl_ms: 2_000,
            path_index_ttl_ms: 5_000,
            path_index_max_roots: 8,
            codehost_ttl_ms: 10_000,
            codehost_max_entries: 256,
            presence_ttl_ms: 500,
            model_health_ttl_ms: 500,
            harness_usage_ttl_ms: 180_000,
            desktop_ports_poll_ms: 5_000,
            desktop_stats_poll_ms: 5_000,
            desktop_disk_poll_ms: 60_000,
            desktop_network_poll_ms: 30_000,
            desktop_sessions_safety_ms: 60_000,
        }
    }
}

impl CacheSettings {
    /// Read the `cache.*` values out of a resolved settings list, falling back
    /// to [`Default`] for any key the list omits or gives a wrong-typed value.
    pub fn from_resolved(resolved: &[Resolved]) -> Self {
        let d = Self::default();
        let u64_of = |key: &str, fallback: u64| {
            resolved
                .iter()
                .find(|r| r.key.as_str() == key)
                .and_then(|r| r.value.as_u64())
                .unwrap_or(fallback)
        };
        let u32_of = |key: &str, fallback: u32| u64_of(key, u64::from(fallback)) as u32;
        let bool_of = |key: &str, fallback: bool| {
            resolved
                .iter()
                .find(|r| r.key.as_str() == key)
                .and_then(|r| r.value.as_bool())
                .unwrap_or(fallback)
        };
        Self {
            enabled: bool_of("cache.enabled", d.enabled),
            harness_listing_ttl_ms: u64_of(
                "cache.harness_listing.ttl_ms",
                d.harness_listing_ttl_ms,
            ),
            harness_models_ttl_ms: u64_of("cache.harness_models.ttl_ms", d.harness_models_ttl_ms),
            git_status_ttl_ms: u64_of("cache.git_status.ttl_ms", d.git_status_ttl_ms),
            path_index_ttl_ms: u64_of("cache.path_index.ttl_ms", d.path_index_ttl_ms),
            path_index_max_roots: u32_of("cache.path_index.max_roots", d.path_index_max_roots),
            codehost_ttl_ms: u64_of("cache.codehost.ttl_ms", d.codehost_ttl_ms),
            codehost_max_entries: u32_of("cache.codehost.max_entries", d.codehost_max_entries),
            presence_ttl_ms: u64_of("cache.presence.ttl_ms", d.presence_ttl_ms),
            model_health_ttl_ms: u64_of("cache.model_health.ttl_ms", d.model_health_ttl_ms),
            harness_usage_ttl_ms: u64_of("cache.harness_usage.ttl_ms", d.harness_usage_ttl_ms),
            desktop_ports_poll_ms: u64_of("cache.desktop.ports_poll_ms", d.desktop_ports_poll_ms),
            desktop_stats_poll_ms: u64_of("cache.desktop.stats_poll_ms", d.desktop_stats_poll_ms),
            desktop_disk_poll_ms: u64_of("cache.desktop.disk_poll_ms", d.desktop_disk_poll_ms),
            desktop_network_poll_ms: u64_of(
                "cache.desktop.network_poll_ms",
                d.desktop_network_poll_ms,
            ),
            desktop_sessions_safety_ms: u64_of(
                "cache.desktop.sessions_safety_ms",
                d.desktop_sessions_safety_ms,
            ),
        }
    }

    /// A TTL from a millisecond field, `ZERO` when caching is disabled.
    fn ttl(&self, ms: u64) -> Duration {
        if self.enabled {
            Duration::from_millis(ms)
        } else {
            Duration::ZERO
        }
    }

    pub fn harness_listing_ttl(&self) -> Duration {
        self.ttl(self.harness_listing_ttl_ms)
    }
    pub fn harness_models_ttl(&self) -> Duration {
        self.ttl(self.harness_models_ttl_ms)
    }
    pub fn git_status_ttl(&self) -> Duration {
        self.ttl(self.git_status_ttl_ms)
    }
    pub fn path_index_ttl(&self) -> Duration {
        self.ttl(self.path_index_ttl_ms)
    }
    pub fn codehost_ttl(&self) -> Duration {
        self.ttl(self.codehost_ttl_ms)
    }
    pub fn presence_ttl(&self) -> Duration {
        self.ttl(self.presence_ttl_ms)
    }
    pub fn model_health_ttl(&self) -> Duration {
        self.ttl(self.model_health_ttl_ms)
    }
    pub fn harness_usage_ttl(&self) -> Duration {
        self.ttl(self.harness_usage_ttl_ms)
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
            CacheSettings::from_resolved(&resolved),
            CacheSettings::default()
        );
    }

    #[test]
    fn disabled_zeroes_every_ttl() {
        let cs = CacheSettings {
            enabled: false,
            ..Default::default()
        };
        assert_eq!(cs.git_status_ttl(), Duration::ZERO);
        assert_eq!(cs.codehost_ttl(), Duration::ZERO);
        assert_eq!(cs.harness_models_ttl(), Duration::ZERO);
    }

    #[test]
    fn enabled_reads_the_millisecond_fields() {
        let cs = CacheSettings::default();
        assert_eq!(cs.git_status_ttl(), Duration::from_millis(2_000));
        assert_eq!(cs.harness_listing_ttl(), Duration::from_millis(30_000));
    }
}
