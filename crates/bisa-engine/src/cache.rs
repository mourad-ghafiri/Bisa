//! The engine's cache configuration and the engine-owned caches.
//!
//! One resolved [`CacheSettings`] snapshot, behind an `RwLock`, that every
//! engine-side cache reads its TTL from; and the code host caches, whose size cap is
//! itself a setting so they are rebuilt when it changes. Loaded when the engine
//! starts and refreshed whenever a `cache.*` setting is written, so a tuning
//! change from the desktop takes effect on the next request.

use std::sync::RwLock;

use bisa_cache::TtlCache;
use bisa_codehost::{CheckRun, PrReviews, PullRequest};
use bisa_core::{CacheSettings, WorkstreamId};
use bisa_harness::UsageState;

/// How many harnesses' usage reports are held — the catalog is a handful.
const HARNESS_USAGE_CAP: usize = 32;

/// The engine's live cache configuration and the caches it owns.
pub struct CacheState {
    settings: RwLock<CacheSettings>,
    /// A workstream's pull request, by id. Rebuilt when the cap setting changes.
    codehost_pr: RwLock<TtlCache<WorkstreamId, Option<PullRequest>>>,
    /// A workstream's check runs, by id.
    codehost_checks: RwLock<TtlCache<WorkstreamId, Vec<CheckRun>>>,
    /// A workstream's PR reviews and threads, by id.
    codehost_reviews: RwLock<TtlCache<WorkstreamId, PrReviews>>,
    /// A harness account's usage, by harness id — a provider's usage
    /// endpoint is not a thing to hit on every render.
    harness_usage: TtlCache<String, UsageState>,
}

impl CacheState {
    pub fn new(settings: CacheSettings) -> Self {
        let cap = settings.codehost_max_entries as usize;
        Self {
            settings: RwLock::new(settings),
            codehost_pr: RwLock::new(TtlCache::with_capacity("codehost.pr", cap)),
            codehost_checks: RwLock::new(TtlCache::with_capacity("codehost.checks", cap)),
            codehost_reviews: RwLock::new(TtlCache::with_capacity("codehost.reviews", cap)),
            harness_usage: TtlCache::with_capacity("harness.usage", HARNESS_USAGE_CAP),
        }
    }

    /// A handle on the harness-usage cache.
    pub fn harness_usage(&self) -> TtlCache<String, UsageState> {
        self.harness_usage.clone()
    }

    /// A snapshot of the current settings. Cheap; read it once per request and
    /// pass TTLs down from it.
    pub fn settings(&self) -> CacheSettings {
        self.settings
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// A handle on the pull-request cache (shares the one store).
    pub fn codehost_pr(&self) -> TtlCache<WorkstreamId, Option<PullRequest>> {
        self.codehost_pr
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// A handle on the check-runs cache.
    pub fn codehost_checks(&self) -> TtlCache<WorkstreamId, Vec<CheckRun>> {
        self.codehost_checks
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// A handle on the PR-reviews cache.
    pub fn codehost_reviews(&self) -> TtlCache<WorkstreamId, PrReviews> {
        self.codehost_reviews
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Drop a workstream's code host entries — after a review, a merge, a new PR,
    /// a resolved thread.
    pub fn invalidate_codehost(&self, id: WorkstreamId) {
        self.codehost_pr().invalidate(&id);
        self.codehost_checks().invalidate(&id);
        self.codehost_reviews().invalidate(&id);
    }

    /// Drop every workstream's code host entries — the accounts changed, so
    /// whose eyes a pull request was read through may no longer be whose it
    /// is read through next.
    pub fn clear_codehost(&self) {
        self.codehost_pr().clear();
        self.codehost_checks().clear();
        self.codehost_reviews().clear();
    }

    /// Replace the settings after a `cache.*` change, rebuilding the code host caches
    /// if their cap changed (a `TtlCache`'s cap is fixed at construction).
    pub fn set(&self, settings: CacheSettings) {
        let new_cap = settings.codehost_max_entries as usize;
        let old_cap = self.settings().codehost_max_entries as usize;
        if new_cap != old_cap {
            // A cap is fixed at construction: the caches are made anew, and
            // what they held is read again on the next ask.
            tracing::info!(target: "bisa_engine::cache", from = old_cap, to = new_cap, "the code host caches were rebuilt for a new cap; what they held is read again");
            *self.codehost_pr.write().unwrap_or_else(|e| e.into_inner()) =
                TtlCache::with_capacity("codehost.pr", new_cap);
            *self
                .codehost_checks
                .write()
                .unwrap_or_else(|e| e.into_inner()) =
                TtlCache::with_capacity("codehost.checks", new_cap);
            *self
                .codehost_reviews
                .write()
                .unwrap_or_else(|e| e.into_inner()) =
                TtlCache::with_capacity("codehost.reviews", new_cap);
        }
        *self.settings.write().unwrap_or_else(|e| e.into_inner()) = settings;
    }
}

/// Refresh the cache settings if `key` is one of ours. Called from the engine's
/// `set_setting` / `unset_setting`, beside the gh-preference refresh.
pub fn refresh_for(inner: &crate::Inner, key: &str) {
    if !key.starts_with("cache.") {
        return;
    }
    let resolved = match inner.ws.settings(None) {
        Ok(resolved) => resolved,
        Err(e) => {
            tracing::warn!(target: "bisa_engine::cache", key, "the cache settings were not read; the ones in force stand: {e}");
            return;
        }
    };
    inner.cache.set(CacheSettings::from_resolved(&resolved));
}
