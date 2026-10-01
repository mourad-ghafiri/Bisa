//! A harness account's usage — what it has left — read from the harness's
//! own source through its adapter, on demand, and cached.
//!
//! The desktop asks when a harness row shows its usage line; nothing here
//! polls a provider on its own. The answer is held for
//! `cache.harness_usage.ttl_ms` (a provider's usage endpoint is not a thing
//! to hit on every render) unless it is a failure, which is asked again
//! next time; a refresh drops the held answer first. `harness.usage.reads`
//! off means nothing is asked at all — every harness answers *off*.
//!
//! A report is percentages, labels and reset times. The adapter's reader
//! held the credential for its one request and dropped it; nothing here
//! sees it, logs it, or writes it anywhere.

use bisa_harness::UsageState;

const READS_KEY: &str = "harness.usage.reads";

/// Whether reads are on — `harness.usage.reads`, `true` by default.
fn reads_on(inner: &crate::Inner) -> bool {
    inner
        .ws
        .settings(None)
        .unwrap_or_default()
        .iter()
        .find(|r| r.key == READS_KEY)
        .and_then(|r| r.value.as_bool())
        .unwrap_or(true)
}

/// The harness's usage: from the cache, else from its adapter.
pub async fn usage(inner: &crate::Inner, harness: &str, refresh: bool) -> UsageState {
    if !reads_on(inner) {
        return UsageState::Off;
    }
    let ttl = inner.cache.settings().harness_usage_ttl();
    let cache = inner.cache.harness_usage();
    let key = harness.to_string();
    if refresh {
        cache.invalidate(&key);
    } else if let Some(hit) = cache.get(ttl, &key) {
        return hit;
    }
    let state = inner.catalog.usage_for(harness).await;
    if !ttl.is_zero() && state.cacheable() {
        cache.insert(key, state.clone());
    }
    state
}
