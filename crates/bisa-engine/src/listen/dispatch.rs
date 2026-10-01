//! The worker: claim a signal, meet its listener's guard, start its run,
//! settle it.
//!
//! **One signal makes one run.** A signal is claimed atomically; its run is
//! made with the signal's id as `dispatched`, which the store holds unique —
//! a dispatch replayed after a crash is refused as already dispatched and
//! settles as done, never a second run. Every exit settles the signal exactly
//! once: `done` (it began its run), `waiting` (its guard holds it behind a
//! live run of its listener), `skipped` (dropped, and why) or `failed`.
//!
//! The guard is decided here, one signal at a time, counted over the
//! listener's **live runs**, never over dispatches: a `queue` listener starts
//! a run when none of its runs is unfinished, `parallel { n }` while fewer
//! than `n` are, `skip` drops an occurrence while one is. A goal runs one of
//! a listener's runs at a time, whatever its guard says. A run of a listener
//! that ends lets its waiting signals go again ([`release_ready`]).

use super::registry::{put_runtime, runtime_of, Armed};
use super::{armed, host_event, now_secs, report_once};
use crate::events::{EnginePayload, FiredOutcome};
use crate::{ops, EngineError, Inner};
use bisa_core::{
    map_event, Admission, Dropped, GoalId, ListenerHost, ListenerKey, PauseReason, Paused,
    RunEntry, Signal,
};
use bisa_store::{QueuedSignal, SignalState, StoreError};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The background worker: claim, guard, start, settle, forever.
pub async fn run_worker(inner: Arc<Inner>) {
    // What a previous process was holding when it stopped goes back first.
    recover_stale(&inner);
    let poll = Duration::from_secs(inner.config.signal_poll_secs.max(1));
    let sweep_every = Duration::from_secs(60);
    let mut last_sweep = Instant::now();
    loop {
        inner.pause.wait_running().await;
        let mut worked = 0;
        crate::survive("signal worker", async {
            if last_sweep.elapsed() >= sweep_every {
                recover_stale(&inner);
                last_sweep = Instant::now();
            }
            worked = drain(&inner);
        })
        .await;
        if worked == 0 {
            inner.listen.rest(poll).await;
        }
    }
}

/// Settle the queue to a standstill: claim and decide until nothing is
/// claimable. Returns how many signals it decided. The test door, and what a
/// surface calls to settle without waiting out `signal_poll_secs`.
pub async fn drain_signals(inner: &Arc<Inner>) -> usize {
    let mut total = 0;
    loop {
        let n = drain(inner);
        total += n;
        if n == 0 {
            break;
        }
        tokio::task::yield_now().await;
    }
    total
}

fn recover_stale(inner: &Arc<Inner>) {
    match inner
        .ws
        .requeue_stale_running(inner.config.signal_stale_secs)
    {
        Ok(ids) if !ids.is_empty() => {
            tracing::info!(target: "bisa_engine::listen", "requeued {} signal(s) a stopped worker was holding", ids.len())
        }
        Ok(_) => {}
        Err(e) => {
            tracing::warn!(target: "bisa_engine::listen", "the signal recovery sweep failed: {e}")
        }
    }
}

/// One pass: every claimable signal decided, one at a time, so a guard's
/// count is exact. `events.enabled` off claims nothing: the queue waits as
/// it is.
fn drain(inner: &Arc<Inner>) -> usize {
    if !inner.listen.settings().enabled {
        return 0;
    }
    let mut decided = 0;
    loop {
        let queued = match inner.ws.claim_next_signal() {
            Ok(Some(q)) => q,
            Ok(None) => break,
            Err(e) => {
                tracing::warn!(target: "bisa_engine::listen", "a signal could not be claimed: {e}");
                break;
            }
        };
        decide(inner, queued);
        decided += 1;
    }
    decided
}

/// What becomes of one claimed signal.
fn decide(inner: &Arc<Inner>, queued: QueuedSignal) {
    let first_claim = queued.attempts <= 1;
    let signal = queued.signal;
    let Some(key) = signal.listener.clone() else {
        // A replay record is never claimed; one that was is settled.
        settle(inner, &signal.id, SignalState::Done, None);
        return;
    };
    let registry = armed(inner);
    let Some(armed) = registry.get(&key) else {
        skip(
            inner,
            &key,
            &signal,
            "its listener is not armed: the host stopped listening, or the start changed or is gone",
        );
        return;
    };
    let live = match inner.ws.live_runs_of_listener(&key) {
        Ok(n) => n as usize,
        Err(e) => {
            fail(
                inner,
                &key,
                &signal,
                format!("its listener's runs could not be counted: {e}"),
            );
            return;
        }
    };
    let now = now_secs();
    let mut rt = runtime_of(inner, armed);
    let since = rt.last_dispatch_at.map(|at| now.saturating_sub(at));
    match admission(armed, live, since, first_claim) {
        Admission::Hold => {
            settle(
                inner,
                &signal.id,
                SignalState::Waiting,
                Some("held behind a run of its listener that is still going"),
            );
        }
        Admission::Drop(why) => {
            let reason = match why {
                Dropped::Debounced { secs_left } => format!(
                    "debounced: within {}s of the last run it started ({secs_left}s left)",
                    armed.guard.debounce_secs
                ),
                Dropped::Busy => {
                    "skipped: its last run is unfinished and it skips while one goes".to_string()
                }
            };
            skip(inner, &key, &signal, &reason);
        }
        Admission::Start => {
            // Debounce counts from the moment a dispatch starts, written before
            // it runs: "it did not work" never lifts the rate limit.
            rt.last_dispatch_at = Some(now);
            put_runtime(inner, &key, &rt);
            start(inner, armed, &signal);
        }
    }
}

/// What one claimed occurrence meets. A goal runs one of a listener's runs at
/// a time, whatever its guard says. The debounce is met once, when the
/// occurrence is first claimed: one that waited its turn — or that a stopped
/// worker was holding — is asked only whether there is room, so neither a
/// wait nor a crash can turn an occurrence that was let in into one dropped.
fn admission(armed: &Armed, live: usize, since: Option<u64>, first_claim: bool) -> Admission {
    if matches!(armed.key.host, ListenerHost::Goal { .. }) && live > 0 {
        return Admission::Hold;
    }
    armed.guard.admit(live, since.filter(|_| first_claim))
}

/// Start the run a signal selects, and settle it by what happened.
fn start(inner: &Arc<Inner>, armed: &Armed, signal: &Signal) {
    let key = &armed.key;
    let inputs = match inputs_for(armed, signal) {
        Ok(inputs) => inputs,
        Err(e) => {
            fail(inner, key, signal, e);
            return;
        }
    };
    let entry = RunEntry::at(key.step.clone(), Some(signal.clone()));
    let started = match key.host {
        ListenerHost::Workspace { workflow } => ops::start_workspace_run(
            inner,
            workflow,
            inputs,
            entry,
            armed.listening.budget.clone(),
            Some(signal.id.clone()),
        )
        .map(|run| (run, None)),
        ListenerHost::Goal { goal } => match goal_may_run(inner, goal) {
            Ok(()) => ops::start_run(inner, goal, inputs, entry, Some(signal.id.clone()))
                .map(|run| (run, Some(goal))),
            Err(reason) => {
                skip(inner, key, signal, &reason);
                return;
            }
        },
    };
    match started {
        Ok((run, goal)) => {
            settle(inner, &signal.id, SignalState::Done, None);
            super::healthy(inner, key);
            fired(
                inner,
                key,
                signal,
                FiredOutcome::Started { run: run.id, goal },
            );
        }
        // A dispatch replayed after a crash: the run it began stands.
        Err(EngineError::Store(StoreError::AlreadyDispatched { .. })) => {
            settle(
                inner,
                &signal.id,
                SignalState::Done,
                Some("already began its run"),
            );
        }
        Err(e) if e.is_refusal() => fail(inner, key, signal, e.to_string()),
        Err(e) => fail(inner, key, signal, format!("its run could not start: {e}")),
    }
}

/// The inputs one occurrence gives its run: what the host listens with,
/// beneath what the start's mapping reads off the event.
fn inputs_for(
    armed: &Armed,
    signal: &Signal,
) -> Result<BTreeMap<String, serde_json::Value>, String> {
    let event = serde_json::to_value(signal).map_err(|e| e.to_string())?;
    let mapped = map_event(&armed.mapping, &armed.workflow.inputs, &event)
        .map_err(|e| format!("the event does not fill the run's inputs: {e}"))?;
    let mut inputs = armed.listening.inputs.clone();
    inputs.extend(mapped);
    Ok(inputs)
}

/// Whether a listening goal may start a run now: open, and with budget left.
/// A spent budget pauses its listening — said once — rather than refusing
/// every occurrence after.
fn goal_may_run(inner: &Arc<Inner>, goal: GoalId) -> Result<(), String> {
    let home = bisa_core::Home::Goal { goal };
    match inner.ws.budget_allows(&home) {
        Ok(true) => Ok(()),
        Ok(false) => {
            pause_goal(inner, goal, PauseReason::BudgetSpent);
            Err("the goal's budget is spent; its listening is paused".to_string())
        }
        Err(e) => Err(format!("the goal's budget could not be read: {e}")),
    }
}

/// Pause a goal's listening: nothing more starts until a repair is adopted
/// or a person says *listen again*; its waiting and queued signals settle.
pub(crate) fn pause_goal(inner: &Arc<Inner>, goal: GoalId, reason: PauseReason) {
    let host = ListenerHost::Goal { goal };
    let mut listening = match inner.ws.listening(&host) {
        Ok(Some(listening)) => listening,
        // It does not listen: there is nothing to pause.
        Ok(None) => return,
        Err(e) => {
            tracing::warn!(target: "bisa_engine::listen", %goal, "what the goal listens with could not be read; its listening is not paused: {e}");
            return;
        }
    };
    if listening.is_paused() {
        return;
    }
    let words = match &reason {
        PauseReason::RunFailed { run } => format!("a run it started failed ({run})"),
        PauseReason::BudgetSpent => "its budget is spent".to_string(),
    };
    listening.paused = Some(Paused {
        reason,
        at: now_secs(),
    });
    if let Err(e) = inner.ws.set_listening(&host, Some(listening)) {
        tracing::warn!(target: "bisa_engine::listen", %goal, "the goal's listening could not be paused: {e}");
        return;
    }
    crate::warn_on_err(
        inner.ws.settle_pending_signals(
            &host,
            SignalState::Skipped,
            &format!("the goal's listening paused: {words}"),
        ),
        "settling a paused goal's signals",
    );
    // Its queued event runs are withdrawn: the repair has a gap, and a
    // person's own queued run stays in line.
    for queued in inner.ws.queued_runs(goal).unwrap_or_default() {
        if queued.dispatched.is_some() {
            // One that started meanwhile is the goal's live run now, and
            // goes on: a withdrawal takes only what is still in the queue.
            crate::warn_on_err(
                ops::withdraw_if_queued(inner, queued.id),
                "withdrawing a paused goal's queued event run",
            );
        }
    }
    super::invalidate(inner);
    inner.emit(crate::events::EngineEvent::scoped(
        goal,
        None,
        EnginePayload::ListeningChanged { host, on: false },
    ));
    ops::journal_note_as(
        inner,
        None,
        bisa_core::Home::Goal { goal },
        format!("listening paused — {words}; adopt a repair or listen again"),
    );
}

/// Let a listener's waiting signals go as far as its guard has room — after
/// a run of it ended, and on every tick. A signal waiting behind a run goes
/// when the run count allows; one over the rate when the window does. A
/// listener no longer armed lets them all go, to be skipped on claim. Room
/// is counted over live runs and the signals already queued, so nothing
/// goes back only to wait again.
pub(crate) fn release_ready(inner: &Arc<Inner>, key: &ListenerKey) {
    let pending = match inner.ws.pending_signals(key) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(target: "bisa_engine::listen", listener = %key, "its waiting signals could not be read: {e}");
            return;
        }
    };
    let waiting: Vec<&QueuedSignal> = pending
        .iter()
        .filter(|q| q.state == SignalState::Waiting)
        .collect();
    if waiting.is_empty() {
        return;
    }
    let registry = armed(inner);
    let room = match registry.get(key) {
        None => waiting.len(),
        Some(armed) => {
            let live = inner.ws.live_runs_of_listener(key).unwrap_or(u32::MAX) as usize;
            let queued = pending
                .iter()
                .filter(|q| matches!(q.state, SignalState::Queued | SignalState::Running))
                .count();
            let busy = live.saturating_add(queued);
            match (armed.key.host, armed.guard.overlap) {
                (ListenerHost::Workspace { .. }, bisa_core::Overlap::Parallel(max)) => {
                    (max.get() as usize).saturating_sub(busy)
                }
                _ => usize::from(busy == 0),
            }
        }
    };
    let now = now_secs();
    for q in waiting.into_iter().take(room) {
        let over_rate = q
            .note
            .as_deref()
            .is_some_and(|n| n.starts_with(super::OVER_RATE));
        if over_rate && !inner.listen.allow_fire(key, now) {
            continue;
        }
        crate::warn_on_err(
            inner.ws.release_signal(&q.signal.id),
            "letting a waiting signal go",
        );
        inner.listen.wake();
    }
}

fn settle(inner: &Inner, signal: &str, state: SignalState, note: Option<&str>) {
    if let Err(e) = inner.ws.move_signal(signal, state, note) {
        tracing::warn!(target: "bisa_engine::listen", %signal, "a signal could not be settled: {e}");
    }
}

fn skip(inner: &Arc<Inner>, key: &ListenerKey, signal: &Signal, reason: &str) {
    tracing::debug!(target: "bisa_engine::listen", listener = %key, signal = %signal.id, "{reason}");
    settle(inner, &signal.id, SignalState::Skipped, Some(reason));
    fired(
        inner,
        key,
        signal,
        FiredOutcome::Skipped {
            reason: reason.to_string(),
        },
    );
}

fn fail(inner: &Arc<Inner>, key: &ListenerKey, signal: &Signal, error: String) {
    settle(inner, &signal.id, SignalState::Failed, Some(&error));
    report_once(inner, key, Some(&signal.id), error);
}

fn fired(inner: &Inner, key: &ListenerKey, signal: &Signal, outcome: FiredOutcome) {
    inner.emit(host_event(
        inner,
        key,
        EnginePayload::ListenerFired {
            listener: key.clone(),
            signal: signal.id.clone(),
            outcome,
        },
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use bisa_core::{
        Chain, Guard, InputDef, InputKind, InputName, Listening, SignalScope, SignalSource,
        StartOn, Workflow, WorkflowId,
    };

    fn armed(mapping: &[(&str, &str)], inputs: Vec<InputDef>, listening: &[(&str, &str)]) -> Armed {
        Armed {
            key: format!(
                "workspace:{}/ticket",
                WorkflowId::from_ulid(ulid::Ulid::from_parts(1, 1))
            )
            .parse()
            .unwrap(),
            workflow: Arc::new(Workflow {
                id: WorkflowId::from_ulid(ulid::Ulid::from_parts(1, 1)),
                name: "w".into(),
                description: String::new(),
                inputs,
                steps: vec![],
                origin: bisa_core::WorkflowOrigin::Workspace,
                author: bisa_core::PrincipalId::new("a".repeat(64)).unwrap(),
                tags: Default::default(),
                revision: 1,
                archived: None,
                decision_making: false,
                created_at: 0,
            }),
            on: StartOn::Hook { public: false },
            mapping: mapping
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            guard: Guard::default(),
            listening: Listening {
                inputs: listening
                    .iter()
                    .map(|(k, v)| (k.to_string(), serde_json::json!(v)))
                    .collect(),
                budget: None,
                since: 0,
                paused: None,
            },
            digest: String::new(),
        }
    }

    fn input(name: &str, required: bool) -> InputDef {
        InputDef {
            name: InputName::new(name).unwrap(),
            label: name.into(),
            kind: InputKind::Text,
            default: None,
            required,
        }
    }

    fn signal(payload: serde_json::Value) -> Signal {
        Signal {
            id: "s1".into(),
            listener: None,
            source: SignalSource::Hook,
            name: None,
            at: 1,
            payload,
            scope: SignalScope::Workspace,
            chain: Chain::default(),
            dedupe_key: None,
        }
    }

    /// An occurrence meets the debounce when it is first claimed, never
    /// again; a goal holds whatever the guard would let through.
    #[test]
    fn the_debounce_is_met_once_and_a_goal_runs_one_at_a_time() {
        let mut library = armed(&[], vec![], &[]);
        library.guard = Guard {
            debounce_secs: 60,
            ..Guard::default()
        };
        assert_eq!(
            admission(&library, 0, Some(10), true),
            Admission::Drop(Dropped::Debounced { secs_left: 50 })
        );
        assert_eq!(admission(&library, 0, Some(10), false), Admission::Start);
        assert_eq!(admission(&library, 1, Some(10), false), Admission::Hold);
        assert_eq!(admission(&library, 0, None, true), Admission::Start);

        let mut skipping = armed(&[], vec![], &[]);
        skipping.guard = Guard {
            overlap: bisa_core::Overlap::Skip,
            ..Guard::default()
        };
        assert_eq!(
            admission(&skipping, 1, None, false),
            Admission::Drop(Dropped::Busy)
        );

        let mut goal = armed(&[], vec![], &[]);
        goal.key = format!(
            "goal:{}/ticket",
            GoalId::from_ulid(ulid::Ulid::from_parts(2, 2))
        )
        .parse()
        .unwrap();
        goal.guard = Guard {
            overlap: bisa_core::Overlap::Parallel(std::num::NonZeroU32::new(4).unwrap()),
            ..Guard::default()
        };
        assert_eq!(admission(&goal, 1, None, true), Admission::Hold);
        assert_eq!(admission(&goal, 0, None, true), Admission::Start);
    }

    /// The listening inputs lie beneath; the mapping reads the event over
    /// them — a run by an event binds what a run by hand would.
    #[test]
    fn an_occurrences_inputs_are_the_listening_ones_under_the_mapping() {
        let a = armed(
            &[("ticket", "{event.payload.ticket}")],
            vec![input("ticket", true), input("channel", false)],
            &[("channel", "support"), ("ticket", "from listening")],
        );
        let inputs = inputs_for(
            &a,
            &signal(serde_json::json!({"ticket": "printer on fire"})),
        )
        .unwrap();
        assert_eq!(inputs["ticket"], serde_json::json!("printer on fire"));
        assert_eq!(inputs["channel"], serde_json::json!("support"));
        let missing = inputs_for(&a, &signal(serde_json::json!({}))).unwrap_err();
        assert!(missing.contains("ticket"), "{missing}");
    }
}
