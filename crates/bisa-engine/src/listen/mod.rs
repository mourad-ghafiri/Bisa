//! Listening: the runtime that hears start events and turns each occurrence
//! into a run — and the ear that tells a run's waits and boundary events
//! what happened.
//!
//! **An event never acts. It enqueues.** Every occurrence — a schedule that
//! came due, a hook call, a message, a named signal, a project's change, a
//! run's end, a platform topic, a new item a poll found, a check that started
//! failing — is written down as a durable [`bisa_core::Signal`] for the
//! listener it is for before anything moves, and a separate worker starts
//! runs from the queue under the same gates, budgets, pause switch and caps
//! as a person's start. A flood costs the queue, not the workspace.
//!
//! ```text
//! sources (ticker) ─┐
//! ear (the bus)    ─┼─ offer ─► waits + boundaries (a run holding for it)
//! messages         ─┤         └► a signal per listener that hears it
//! emit, hooks      ─┘                    │  durable, deduplicated
//!                                        ▼
//!                              dispatch: guard over live runs → a run
//! ```
//!
//! - [`registry`] — the listeners armed right now: a projection of the
//!   listening hosts, each start resolved against its host's inputs.
//! - [`schedule`] — when a cadence next comes due.
//! - [`sources`] — the ticker: schedules, polls, checks, projects.
//! - [`ear`] — the one ear: the engine's bus and the conversation door, heard
//!   once and offered to waits, boundaries and listeners alike.
//! - [`emit`] — the one door a named signal is raised through.
//! - [`hooks`] — a hook call: local, or public with its secret.
//! - [`dispatch`] — the worker: claim, guard, start, settle.
//! - [`turn`] — turning a host on and off, its hook secrets shown once.
//!
//! The loop guard is the causal [`bisa_core::Chain`]: a signal carries the
//! listeners that led to it, a run keeps its chain, and a listener already in
//! the chain — or one hop past `events.chain_depth` — refuses. Every start but
//! a schedule's, a hook's and a person's is held to `events.fires_per_minute`.

pub mod dispatch;
pub mod ear;
pub mod emit;
pub mod hooks;
pub mod registry;
pub mod schedule;
pub mod sources;
pub mod turn;

use crate::Inner;
use bisa_core::{EventSettings, ListenerKey, ProjectId, RunId};
use dashmap::DashMap;
use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub(crate) fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// The window `events.fires_per_minute` counts over.
pub const RATE_WINDOW_SECS: u64 = 60;

/// How many message and commit ids the loop guard remembers the run of.
const RUNS_REMEMBERED: usize = 512;

/// The note a signal over its listener's rate waits under — the one kind of
/// waiting signal a window lets go, not a run's end.
pub(crate) const OVER_RATE: &str = "over events.fires_per_minute";

/// The listening runtime's state, owned by [`crate::Inner`].
pub struct ListenState {
    /// The `events.*` settings in force — seeded at start, replaced by
    /// [`refresh_for`] when one is written.
    settings: RwLock<EventSettings>,
    /// The armed listeners, built on first need and dropped by
    /// [`invalidate`] whenever what they are built from may have moved.
    registry: RwLock<Option<Arc<registry::Registry>>>,
    /// Recent fires per listener, for `events.fires_per_minute`.
    fires: DashMap<ListenerKey, Vec<u64>>,
    /// What each listener last reported as wrong with it, so a failure is
    /// said once, not every tick. Kept beside the listener's memory too
    /// (`ListenerRuntime::failed`), so a surface that lists the listeners
    /// says it after the frame is gone.
    reported: DashMap<ListenerKey, String>,
    /// Rings when a signal became claimable, so the worker claims it then
    /// and not at its next poll.
    wake: tokio::sync::Notify,
    /// A message a run's work posted → that run: a message start that hears
    /// it inherits the run's chain.
    message_runs: Recent,
    /// A commit a run's work made → that run: a project start that sees it
    /// on a branch inherits the run's chain.
    commit_runs: Recent,
    /// A workstream moved: pull request states are asked on the next tick
    /// rather than at the poll's cadence.
    pub(crate) prs_due: std::sync::atomic::AtomicBool,
    /// When each project's pull requests were last asked of its code host,
    /// and what it answered.
    pub(crate) prs_polled: DashMap<ProjectId, u64>,
    pub(crate) prs_seen: DashMap<ProjectId, BTreeMap<String, sources::PullRequestSeen>>,
    /// The ticker's own last look at each project a wait holds for: what the
    /// next look is compared with. Memory only — a restart looks afresh.
    pub(crate) project_views: DashMap<ProjectId, sources::ProjectView>,
}

impl Default for ListenState {
    fn default() -> Self {
        Self::with_settings(EventSettings::default())
    }
}

impl ListenState {
    pub fn with_settings(settings: EventSettings) -> Self {
        Self {
            settings: RwLock::new(settings),
            registry: RwLock::new(None),
            fires: DashMap::new(),
            reported: DashMap::new(),
            wake: tokio::sync::Notify::new(),
            message_runs: Recent::default(),
            commit_runs: Recent::default(),
            prs_due: std::sync::atomic::AtomicBool::new(false),
            prs_polled: DashMap::new(),
            prs_seen: DashMap::new(),
            project_views: DashMap::new(),
        }
    }

    /// A snapshot of the settings in force.
    pub fn settings(&self) -> EventSettings {
        self.settings
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn set_settings(&self, settings: EventSettings) {
        *self.settings.write().unwrap_or_else(|e| e.into_inner()) = settings;
    }

    /// Count one fire of `key` and say whether it stayed under the ceiling.
    pub(crate) fn allow_fire(&self, key: &ListenerKey, now: u64) -> bool {
        let ceiling = self.settings().fires_per_minute.max(1) as usize;
        let mut fires = self.fires.entry(key.clone()).or_default();
        fires.retain(|t| now.saturating_sub(*t) < RATE_WINDOW_SECS);
        if fires.len() >= ceiling {
            return false;
        }
        fires.push(now);
        true
    }

    /// A signal became claimable. One ring is kept when the worker is busy,
    /// so none is lost between two of its passes.
    pub(crate) fn wake(&self) {
        self.wake.notify_one();
    }

    /// The worker's rest between two passes: until a signal becomes
    /// claimable, and no longer than `poll` — the bound that finds what no
    /// ring announced.
    pub(crate) async fn rest(&self, poll: Duration) {
        tokio::select! {
            _ = self.wake.notified() => {}
            _ = tokio::time::sleep(poll) => {}
        }
    }

    /// What is wrong with `key`, when something was said and not yet cleared.
    pub(crate) fn trouble_of(&self, key: &ListenerKey) -> Option<String> {
        self.reported.get(key).map(|e| e.value().clone())
    }

    /// Forget what was said and counted for every listener of `host`: it was
    /// turned on anew, or it stopped listening.
    pub(crate) fn forget_host(&self, host: &bisa_core::ListenerHost) {
        self.reported.retain(|key, _| key.host != *host);
        self.fires.retain(|key, _| key.host != *host);
    }
}

/// Say a listener's trouble once: the first time, and again only when it
/// changes. A listener that works again clears it ([`healthy`]).
pub(crate) fn report_once(
    inner: &Inner,
    key: &ListenerKey,
    signal: Option<&str>,
    error: String,
) -> bool {
    let fresh = inner
        .listen
        .reported
        .insert(key.clone(), error.clone())
        .as_deref()
        != Some(error.as_str());
    if fresh {
        tracing::warn!(target: "bisa_engine::listen", listener = %key, "{error}");
        keep_trouble(inner, key);
        let payload = crate::events::EnginePayload::ListenerFailed {
            listener: key.clone(),
            signal: signal.map(str::to_string),
            error,
        };
        inner.emit(host_event(inner, key, payload));
    }
    fresh
}

/// A listener that worked has nothing outstanding to report.
pub(crate) fn healthy(inner: &Inner, key: &ListenerKey) {
    if inner.listen.reported.remove(key).is_some() {
        keep_trouble(inner, key);
    }
}

/// Write what is wrong with a listener — or that nothing is — beside its
/// memory. [`registry::put_runtime`] stamps it on every write, so the record
/// never says more or less than was last said.
pub(crate) fn keep_trouble(inner: &Inner, key: &ListenerKey) {
    let rt = inner.ws.listener_runtime(key);
    if rt.failed != inner.listen.trouble_of(key) {
        registry::put_runtime(inner, key, &rt);
    }
}

/// What an event about a listener says it is about: its goal, or its
/// workflow when the workspace listens.
pub(crate) fn host_event(
    _inner: &Inner,
    key: &ListenerKey,
    payload: crate::events::EnginePayload,
) -> crate::events::EngineEvent {
    match key.host {
        bisa_core::ListenerHost::Goal { goal } => {
            crate::events::EngineEvent::scoped(goal, None, payload)
        }
        bisa_core::ListenerHost::Workspace { workflow } => crate::events::EventScope {
            goal: None,
            workflow: Some(workflow),
            run: None,
        }
        .event(None, payload),
    }
}

/// Refresh the `events.*` settings when `key` is one of them. Called from the
/// engine's settings write, beside every other module that holds a setting.
pub fn refresh_for(inner: &Inner, key: &str) {
    if !key.starts_with("events.") {
        return;
    }
    match inner.ws.settings(None) {
        Ok(resolved) => inner
            .listen
            .set_settings(EventSettings::from_resolved(&resolved)),
        Err(e) => {
            tracing::warn!(
                key,
                "the events settings were not read; the ones in force stand: {e}"
            );
        }
    }
    invalidate(inner);
}

/// Drop the armed listeners: they are built again, from the listening
/// records and the definitions, the next time anything asks.
pub fn invalidate(inner: &Inner) {
    *inner
        .listen
        .registry
        .write()
        .unwrap_or_else(|e| e.into_inner()) = None;
}

/// The listeners armed right now, built when nothing is cached.
pub fn armed(inner: &Arc<Inner>) -> Arc<registry::Registry> {
    if let Some(cached) = inner
        .listen
        .registry
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
    {
        return cached;
    }
    let built = Arc::new(registry::build(inner));
    *inner
        .listen
        .registry
        .write()
        .unwrap_or_else(|e| e.into_inner()) = Some(Arc::clone(&built));
    built
}

/// Post a message a run's work makes, remembered for the loop guard before
/// anything can hear it: the memory is held while `post` writes the message,
/// so the ear — which asks [`run_of_message`] — finds the run whenever it
/// asks. A post nobody's run made is made as it is.
pub(crate) fn post_for_run<E>(
    inner: &Inner,
    run: Option<RunId>,
    post: impl FnOnce() -> Result<String, E>,
) -> Result<String, E> {
    match run {
        Some(run) => inner.listen.message_runs.insert_made(run, post),
        None => post(),
    }
}

pub(crate) fn run_of_message(inner: &Inner, message: &str) -> Option<RunId> {
    inner.listen.message_runs.get(message)
}

/// A commit a run's work made, remembered for the loop guard.
pub(crate) fn note_commit_run(inner: &Inner, sha: &str, run: RunId) {
    inner.listen.commit_runs.insert(sha, run);
}

pub(crate) fn run_of_commit(inner: &Inner, sha: &str) -> Option<RunId> {
    inner.listen.commit_runs.get(sha)
}

/// The causal chain of a run, when it can be read.
pub(crate) fn chain_of_run(inner: &Inner, run: RunId) -> bisa_core::Chain {
    inner.ws.get_run(run).map(|r| r.chain).unwrap_or_default()
}

/// A bounded memory of `id → run`, oldest forgotten first.
#[derive(Default)]
struct Recent {
    entries: Mutex<VecDeque<(String, RunId)>>,
}

impl Recent {
    fn insert(&self, id: &str, run: RunId) {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        Self::remember(&mut entries, id, run);
    }

    /// Remember what `make` makes as `run`'s, holding the memory while it is
    /// made: whoever asks for the id it answers waits until it is remembered.
    fn insert_made<E>(
        &self,
        run: RunId,
        make: impl FnOnce() -> Result<String, E>,
    ) -> Result<String, E> {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        let id = make()?;
        Self::remember(&mut entries, &id, run);
        Ok(id)
    }

    fn remember(entries: &mut VecDeque<(String, RunId)>, id: &str, run: RunId) {
        entries.retain(|(k, _)| k != id);
        entries.push_back((id.to_string(), run));
        while entries.len() > RUNS_REMEMBERED {
            entries.pop_front();
        }
    }

    fn get(&self, id: &str) -> Option<RunId> {
        self.entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .rev()
            .find(|(k, _)| k == id)
            .map(|(_, run)| *run)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(step: &str) -> ListenerKey {
        format!(
            "workspace:{}/{step}",
            bisa_core::WorkflowId::from_ulid(ulid::Ulid::from_parts(1, 1))
        )
        .parse()
        .unwrap()
    }

    #[test]
    fn the_rate_ceiling_counts_a_minute_per_listener() {
        let state = ListenState::with_settings(EventSettings {
            fires_per_minute: 2,
            ..EventSettings::default()
        });
        let a = key("a");
        let b = key("b");
        assert!(state.allow_fire(&a, 100));
        assert!(state.allow_fire(&a, 110));
        assert!(!state.allow_fire(&a, 120), "a third within the minute");
        assert!(state.allow_fire(&b, 120), "another listener's own ceiling");
        assert!(state.allow_fire(&a, 161), "the window moved past the first");
    }

    #[test]
    fn the_run_memory_is_bounded_and_answers_the_newest() {
        let recent = Recent::default();
        let run = |n: u64| RunId::from_ulid(ulid::Ulid::from_parts(n, 1));
        recent.insert("m1", run(1));
        recent.insert("m1", run(2));
        assert_eq!(recent.get("m1"), Some(run(2)));
        for n in 0..(RUNS_REMEMBERED as u64 + 5) {
            recent.insert(&format!("x{n}"), run(n));
        }
        assert_eq!(recent.get("m1"), None, "the oldest are forgotten");
        assert_eq!(recent.get("x4"), None);
        assert!(recent.get(&format!("x{}", RUNS_REMEMBERED + 4)).is_some());
    }

    #[test]
    fn what_a_run_made_is_remembered_by_the_id_it_was_given() {
        let recent = Recent::default();
        let run = RunId::from_ulid(ulid::Ulid::from_parts(7, 1));
        let made: Result<String, String> = recent.insert_made(run, || Ok("m9".to_string()));
        assert_eq!(made.as_deref(), Ok("m9"));
        assert_eq!(recent.get("m9"), Some(run));
        let failed: Result<String, String> = recent.insert_made(run, || Err("refused".into()));
        assert_eq!(failed, Err("refused".to_string()));
        assert_eq!(recent.get("refused"), None, "nothing made, nothing kept");
    }
}
