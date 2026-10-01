//! The model-health ledger — what the engine knows, right now, about which
//! `(harness, model)` pairs will actually run.
//!
//! [`bisa_harness::ModelPlan::order`] is a pure function of
//! `(plan, health, rotation)`. This module is the `health` half: the live,
//! observed side of a model plan, fed by every launch and every session that
//! hit a wall. Nothing here decides an order — [`ModelLedger::view`] hands a
//! [`ModelHealthView`] to `order()` and the plan's own strategy does the rest.
//!
//! # Keyed on the pair, not on the model
//!
//! "fable-5 is rate-limited" is never true on its own. It is true of *this
//! harness's* account, credentials and quota: Claude Code and a Codex install
//! pointed at a different org can hold entirely different opinions about the
//! same model id, and a harness that has simply never heard of a model id is
//! saying something about itself. So the ledger keys on `(harness, model)`,
//! and a plan is only ever ordered through a view scoped to one harness.
//!
//! The effort a session runs at is no part of the key: a wall is the
//! account's, whatever level the model was asked to work at.
//!
//! A session with no model pinned still has a key: `"<harness> default"`,
//! which is exactly the name `bisa_adapters::util::ModelCtx::model_name`
//! reports for it. "The harness default went away" is a real fact, and it is
//! the one thing an unpinned agent can learn.
//!
//! # In-memory, per-process, and deliberately so
//!
//! The ledger is a [`DashMap`] on `Inner` alongside the other runtime state,
//! and a restart forgets every cooldown. That is correct rather than
//! convenient: a cooldown is a *guess* about a remote service we cannot see —
//! a `retry_after` the provider offered, or our own backoff standing in for
//! one — and a restart is a perfectly good moment to stop guessing and go
//! ask. The cost of being wrong is a single launch; the cost of persisting a
//! stale guess is an agent that refuses its best model for an hour after the
//! quota actually came back. Nothing here is a fact that belongs in GEP or in
//! the store, and nothing here is a fact another node could use: it describes
//! this process's credentials on this machine at this minute.
//!
//! # The backoff, and its two ends
//!
//! When the harness told us when to come back ([`Outcome::ModelUnavailable`]'s
//! `retry_after`), we use that number: the provider knows the shape of its own
//! window and we do not. Otherwise the cooldown doubles per consecutive
//! failure between two bounds:
//!
//! - **Floor [`COOLDOWN_FLOOR_SECS`] (30s).** A one-second cooldown is
//!   useless. The two things that produce a model wall are per-minute rate
//!   limits and exhausted quota; neither can plausibly have cleared inside a
//!   second, and re-probing costs a real process spawn (~250ms for a trivial
//!   script, considerably more for a harness that has to authenticate). 30s is
//!   the shortest interval over which a per-minute token bucket can actually
//!   have refilled, so it is the shortest interval where a retry can *learn*
//!   anything.
//! - **Ceiling [`COOLDOWN_CEILING_SECS`] (1h).** Beyond an hour the guess is
//!   worthless — a daily quota reset is not something we can time, and being
//!   wrong for the rest of the day is far more expensive than one wasted
//!   launch per hour. An hour is also the cap applied to a `retry_after` the
//!   harness supplied: honouring a provider's "come back in six hours"
//!   literally would lock a model out long after a plan change, a re-auth or a
//!   plan upgrade had fixed it.
//!
//! A **success clears everything immediately** — no decay, no half-life. A
//! model that just ran a session is healthy; pretending otherwise would keep
//! demoting a working model behind one that is not, which is the failure the
//! whole taxonomy exists to remove.

use bisa_harness::ModelHealthView;
use dashmap::DashMap;
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Shortest cooldown the engine will impose on its own. See the module docs.
pub const COOLDOWN_FLOOR_SECS: u64 = 30;

/// Longest cooldown, including one a harness asked for. See the module docs.
pub const COOLDOWN_CEILING_SECS: u64 = 3600;

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// The ledger's name for a session's model: the pinned id, or the honest
/// `"<harness> default"` for an unpinned one.
///
/// This is the same string `bisa_adapters::util::ModelCtx::model_name`
/// puts in a [`bisa_harness::Outcome::ModelUnavailable`], so what the
/// engine records under is what the adapter reported.
pub fn model_key(harness: &str, model: Option<&str>) -> String {
    match model {
        Some(m) if !m.trim().is_empty() => m.to_string(),
        _ => format!("{harness} default"),
    }
}

#[derive(Debug, Default, Clone)]
struct Entry {
    /// Unix seconds. `None` (or a past value) means usable now.
    cooldown_until: Option<u64>,
    consecutive_failures: u32,
    in_flight: u32,
}

impl Entry {
    fn cooling(&self, now: u64) -> Option<u64> {
        self.cooldown_until.filter(|until| *until > now)
    }

    /// Nothing left to remember about this pair.
    fn is_boring(&self, now: u64) -> bool {
        self.in_flight == 0 && self.consecutive_failures == 0 && self.cooling(now).is_none()
    }
}

/// One row of the ledger, for a surface that wants to render "rate-limited,
/// retry in 4m".
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModelHealthRow {
    pub harness: String,
    pub model: String,
    /// Unix seconds, only while still in the future.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cooldown_until: Option<u64>,
    /// Seconds from now until the cooldown expires — what a badge renders.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_in_secs: Option<u64>,
    pub consecutive_failures: u32,
    pub in_flight: u32,
}

impl ModelHealthRow {
    pub fn is_cooling(&self) -> bool {
        self.cooldown_until.is_some()
    }
}

struct Shared {
    entries: DashMap<(String, String), Entry>,
    /// Monotonic launch counter handed to [`bisa_harness::ModelPlan::order`]
    /// as its `rotation`, which is how `RoundRobin` and `Weighted` stay
    /// deterministic without a clock or an RNG. One counter for the process:
    /// two agents rotating through it interleave, and each still rotates.
    rotation: AtomicU64,
    /// The health rows, memoized for `cache.model_health.ttl_ms` —
    /// the read the health endpoint renders from.
    snapshot_cache: bisa_cache::TtlCell<Vec<ModelHealthRow>>,
}

impl Default for Shared {
    fn default() -> Self {
        Self {
            entries: DashMap::new(),
            rotation: AtomicU64::new(0),
            snapshot_cache: bisa_cache::TtlCell::new("engine.model_health"),
        }
    }
}

/// The live model-health ledger. Cheap to clone (one `Arc`), because the
/// in-flight guard has to outlive the borrow that created it.
#[derive(Clone, Default)]
pub struct ModelLedger {
    shared: Arc<Shared>,
}

impl std::fmt::Debug for ModelLedger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModelLedger")
            .field("entries", &self.shared.entries.len())
            .finish()
    }
}

impl ModelLedger {
    pub fn new() -> Self {
        Self::default()
    }

    /// The next rotation value. Take **one per run**, not one per attempt: a
    /// `RoundRobin` plan is supposed to advance across work items, and the
    /// within-run walk already advances by way of the cooldowns this run just
    /// wrote.
    pub fn next_rotation(&self) -> u64 {
        self.shared.rotation.fetch_add(1, Ordering::Relaxed)
    }

    /// Record a model wall and return the unix second the cooldown expires.
    ///
    /// `retry_after` is used verbatim (clamped to [`COOLDOWN_CEILING_SECS`])
    /// when the harness supplied one; otherwise the backoff doubles from
    /// [`COOLDOWN_FLOOR_SECS`] on `consecutive_failures`.
    pub fn note_unavailable(&self, harness: &str, model: &str, retry_after: Option<u64>) -> u64 {
        let now = now_secs();
        let key = (harness.to_string(), model.to_string());
        let mut entry = self.shared.entries.entry(key).or_default();
        entry.consecutive_failures = entry.consecutive_failures.saturating_add(1);
        let wait = match retry_after {
            // The provider's own number. Zero is a harness saying "right
            // away", which we still refuse to take literally — one immediate
            // relaunch into the same wall helps nobody.
            Some(secs) => secs.clamp(1, COOLDOWN_CEILING_SECS),
            None => backoff_secs(entry.consecutive_failures),
        };
        let until = now.saturating_add(wait);
        entry.cooldown_until = Some(until);
        until
    }

    /// A session ran. The pair is healthy again *now*.
    pub fn note_success(&self, harness: &str, model: &str) {
        let key = (harness.to_string(), model.to_string());
        let now = now_secs();
        if let Some(mut entry) = self.shared.entries.get_mut(&key) {
            entry.consecutive_failures = 0;
            entry.cooldown_until = None;
            if entry.is_boring(now) {
                drop(entry);
                self.shared.entries.remove_if(&key, |_, e| e.is_boring(now));
            }
        }
    }

    /// Claim one in-flight slot on this pair. The returned guard releases it
    /// on drop, so a panicking task cannot leak a count and quietly demote a
    /// perfectly good model forever under `LeastBusy`.
    #[must_use = "dropping the guard immediately releases the slot"]
    pub fn acquire(&self, harness: &str, model: &str) -> InFlight {
        let key = (harness.to_string(), model.to_string());
        self.shared
            .entries
            .entry(key.clone())
            .or_default()
            .in_flight += 1;
        InFlight {
            shared: Arc::clone(&self.shared),
            key,
        }
    }

    /// Cooldown deadline for a pair, or `None` when it is usable now. An
    /// expired cooldown reports `None` and is forgotten — `ModelPlan::order`
    /// reads no clock, so a stale `Some(..)` would read as "still cooling".
    pub fn cooldown_until(&self, harness: &str, model: &str) -> Option<u64> {
        let now = now_secs();
        let key = (harness.to_string(), model.to_string());
        let cooling = self.shared.entries.get(&key)?.cooling(now);
        if cooling.is_none() {
            self.shared.entries.remove_if(&key, |_, e| e.is_boring(now));
        }
        cooling
    }

    pub fn in_flight(&self, harness: &str, model: &str) -> u32 {
        self.shared
            .entries
            .get(&(harness.to_string(), model.to_string()))
            .map(|e| e.in_flight)
            .unwrap_or(0)
    }

    /// A [`ModelHealthView`] scoped to one harness, ready for
    /// [`bisa_harness::ModelPlan::order`].
    pub fn view<'a>(&'a self, harness: &'a str) -> HarnessHealth<'a> {
        HarnessHealth {
            ledger: self,
            harness,
        }
    }

    /// Everything the ledger currently knows, newest trouble first. The read
    /// API a surface renders health badges from; expired cooldowns are
    /// reported as healthy, and pairs with nothing to say are omitted.
    pub fn snapshot(&self) -> Vec<ModelHealthRow> {
        let now = now_secs();
        let mut rows: Vec<ModelHealthRow> = self
            .shared
            .entries
            .iter()
            .filter(|e| !e.value().is_boring(now))
            .map(|e| {
                let ((harness, model), entry) = (e.key().clone(), e.value().clone());
                let cooldown_until = entry.cooling(now);
                ModelHealthRow {
                    harness,
                    model,
                    cooldown_until,
                    retry_in_secs: cooldown_until.map(|until| until.saturating_sub(now)),
                    consecutive_failures: entry.consecutive_failures,
                    in_flight: entry.in_flight,
                }
            })
            .collect();
        rows.sort_by(|a, b| {
            b.cooldown_until
                .cmp(&a.cooldown_until)
                .then_with(|| a.harness.cmp(&b.harness))
                .then_with(|| a.model.cmp(&b.model))
        });
        rows
    }

    /// [`snapshot`](Self::snapshot), memoized for `ttl` (`cache.model_health.ttl_ms`)
    /// — the read the health endpoint renders from.
    pub fn snapshot_cached(&self, ttl: std::time::Duration) -> Vec<ModelHealthRow> {
        self.shared
            .snapshot_cache
            .get_or_insert(ttl, || self.snapshot())
    }
}

/// Doubling backoff between the floor and the ceiling, for the case where the
/// harness offered no `retry_after` of its own.
///
/// 30s, 60s, 2m, 4m, 8m, 16m, 32m, 1h, 1h, …
fn backoff_secs(consecutive_failures: u32) -> u64 {
    let steps = consecutive_failures.saturating_sub(1).min(32);
    COOLDOWN_FLOOR_SECS
        .saturating_mul(1u64 << steps.min(63))
        .min(COOLDOWN_CEILING_SECS)
}

/// One claimed in-flight slot. Releases on drop.
pub struct InFlight {
    shared: Arc<Shared>,
    key: (String, String),
}

impl Drop for InFlight {
    fn drop(&mut self) {
        let now = now_secs();
        if let Some(mut entry) = self.shared.entries.get_mut(&self.key) {
            entry.in_flight = entry.in_flight.saturating_sub(1);
        }
        self.shared
            .entries
            .remove_if(&self.key, |_, e| e.is_boring(now));
    }
}

impl std::fmt::Debug for InFlight {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InFlight").field("key", &self.key).finish()
    }
}

/// The ledger seen through one harness's eyes — the shape
/// [`bisa_harness::ModelPlan::order`] takes.
#[derive(Clone, Copy)]
pub struct HarnessHealth<'a> {
    ledger: &'a ModelLedger,
    harness: &'a str,
}

impl ModelHealthView for HarnessHealth<'_> {
    fn cooldown_until(&self, model: &str) -> Option<u64> {
        self.ledger.cooldown_until(self.harness, model)
    }

    fn in_flight(&self, model: &str) -> u32 {
        self.ledger.in_flight(self.harness, model)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_between_the_floor_and_the_ceiling() {
        assert_eq!(backoff_secs(0), COOLDOWN_FLOOR_SECS);
        assert_eq!(backoff_secs(1), 30);
        assert_eq!(backoff_secs(2), 60);
        assert_eq!(backoff_secs(3), 120);
        assert_eq!(backoff_secs(7), 1920);
        // The eighth doubling would be 3840s, so the ceiling bites here.
        assert_eq!(backoff_secs(8), COOLDOWN_CEILING_SECS);
        // No overflow, no wrap-around, at any failure count.
        assert_eq!(backoff_secs(u32::MAX), COOLDOWN_CEILING_SECS);
    }

    #[test]
    fn a_harness_supplied_retry_after_wins_but_is_capped() {
        let ledger = ModelLedger::new();
        let now = now_secs();
        let until = ledger.note_unavailable("mock", "fable-5", Some(240));
        assert!((239..=241).contains(&(until - now)), "{}", until - now);

        let until = ledger.note_unavailable("mock", "opus-5", Some(86_400));
        assert!(until - now <= COOLDOWN_CEILING_SECS);
    }

    #[test]
    fn the_ledger_keys_on_the_pair_not_the_model() {
        let ledger = ModelLedger::new();
        ledger.note_unavailable("claude-code", "fable-5", Some(600));
        assert!(ledger.cooldown_until("claude-code", "fable-5").is_some());
        // The same model on a different harness is a different account.
        assert!(ledger.cooldown_until("codex", "fable-5").is_none());
    }

    #[test]
    fn success_clears_the_cooldown_and_the_streak_at_once() {
        let ledger = ModelLedger::new();
        ledger.note_unavailable("mock", "fable-5", None);
        ledger.note_unavailable("mock", "fable-5", None);
        assert!(ledger.cooldown_until("mock", "fable-5").is_some());
        ledger.note_success("mock", "fable-5");
        assert_eq!(ledger.cooldown_until("mock", "fable-5"), None);
        // And the streak went with it: the next failure starts at the floor.
        let now = now_secs();
        let until = ledger.note_unavailable("mock", "fable-5", None);
        assert!(until - now <= COOLDOWN_FLOOR_SECS + 1);
    }

    #[test]
    fn an_expired_cooldown_reports_healthy() {
        let ledger = ModelLedger::new();
        ledger.note_unavailable("mock", "fable-5", Some(1));
        // Reach in and expire it rather than sleeping.
        ledger
            .shared
            .entries
            .get_mut(&("mock".into(), "fable-5".into()))
            .expect("entry")
            .cooldown_until = Some(now_secs() - 1);
        assert_eq!(ledger.cooldown_until("mock", "fable-5"), None);
        assert!(!ledger.snapshot().iter().any(|r| r.is_cooling()));
    }

    #[test]
    fn in_flight_counts_and_the_guard_cannot_leak() {
        let ledger = ModelLedger::new();
        {
            let _a = ledger.acquire("mock", "opus-5");
            let _b = ledger.acquire("mock", "opus-5");
            assert_eq!(ledger.in_flight("mock", "opus-5"), 2);
            assert_eq!(ledger.in_flight("mock", "fable-5"), 0);
        }
        assert_eq!(ledger.in_flight("mock", "opus-5"), 0);

        // A panicking task unwinds through the guard.
        let l = ledger.clone();
        let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _g = l.acquire("mock", "opus-5");
            panic!("session task died");
        }));
        assert!(unwound.is_err(), "the task really did panic");
        assert_eq!(ledger.in_flight("mock", "opus-5"), 0);
    }

    #[test]
    fn the_view_is_a_health_view_scoped_to_one_harness() {
        use bisa_harness::{ModelPlan, ModelStrategy};
        let ledger = ModelLedger::new();
        ledger.note_unavailable("mock", "fable-5", Some(600));
        let plan = ModelPlan::fallback(["fable-5", "opus-5"]);
        // Cooling models sort behind ready ones, so opus-5 leads.
        assert_eq!(
            plan.order(&ledger.view("mock"), 0, None),
            vec!["opus-5", "fable-5"]
        );
        // On another harness nothing is cooling, so plan order stands.
        assert_eq!(
            plan.order(&ledger.view("codex"), 0, None),
            vec!["fable-5", "opus-5"]
        );

        // LeastBusy reads the live counts through the same view.
        let _g = ledger.acquire("codex", "fable-5");
        let plan = ModelPlan {
            strategy: ModelStrategy::LeastBusy,
            effort: None,
            models: plan.models,
        };
        assert_eq!(
            plan.order(&ledger.view("codex"), 0, None),
            vec!["opus-5", "fable-5"]
        );
    }

    #[test]
    fn an_unpinned_session_is_keyed_as_the_harness_default() {
        assert_eq!(model_key("omp", None), "omp default");
        assert_eq!(model_key("omp", Some("  ")), "omp default");
        assert_eq!(model_key("omp", Some("acme/turbo-9")), "acme/turbo-9");
    }

    #[test]
    fn the_snapshot_is_what_a_badge_renders() {
        let ledger = ModelLedger::new();
        ledger.note_unavailable("mock", "fable-5", Some(240));
        let _g = ledger.acquire("mock", "opus-5");
        let rows = ledger.snapshot();
        assert_eq!(rows.len(), 2);
        let cooling = rows.iter().find(|r| r.model == "fable-5").expect("row");
        assert_eq!(cooling.consecutive_failures, 1);
        assert!(matches!(cooling.retry_in_secs, Some(s) if (239..=240).contains(&s)));
        let busy = rows.iter().find(|r| r.model == "opus-5").expect("row");
        assert_eq!(busy.in_flight, 1);
        assert!(!busy.is_cooling());
    }

    #[test]
    fn rotation_is_monotonic() {
        let ledger = ModelLedger::new();
        assert_eq!(ledger.next_rotation(), 0);
        assert_eq!(ledger.next_rotation(), 1);
        assert_eq!(ledger.next_rotation(), 2);
    }
}
