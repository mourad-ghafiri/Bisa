//! The one ear.
//!
//! Everything the engine observes reaches the filters as one shape,
//! [`bisa_core::Heard`], through [`offer`]: first to the waits and boundary
//! events of the runs holding for something ([`crate::waits::on_heard`]),
//! then — when `events.enabled` is on — to every armed listener that hears
//! it, each getting a durable signal of its own ([`enqueue_for`]) under the
//! causal chain, the rate ceiling and the backlog bound.
//!
//! Two doors feed it besides the ticker, the hooks and `emit`:
//!
//! - **the bus** ([`spawn`]): a run's end is a `run` event, and every payload
//!   is a `platform` event of its topic. The listening runtime's own topics
//!   are never heard back, so a listener can never feed on its own news.
//! - **a message** ([`on_message`]), heard where conversation dispatch hears
//!   it — after the hold a message from another node waits in — and never an
//!   announcement (a workflow's `notify`).
//!
//! The ear also keeps the registry honest: every event that may change what
//! is armed drops it ([`super::invalidate`]).

use super::registry::Armed;
use super::{armed, chain_of_run, now_secs, report_once};
use crate::events::{EngineEvent, EnginePayload};
use crate::Inner;
use bisa_core::{
    Assignee, CancelCause, Heard, ListenerKey, MessageBody, RunId, RunOutcome, Signal, SignalScope,
    SignalSource, StartOn,
};
use bisa_store::{PostOrigin, SignalState};
use serde_json::{json, Value};
use std::sync::Arc;

/// The listening runtime's own news: never heard back as a `platform` event,
/// or a listener could feed itself on its own fires.
const OWN_TOPICS: &[&str] = &[
    "signal.received",
    "listener.fired",
    "listener.failed",
    "listening.changed",
];

/// A fresh signal id.
pub(crate) fn new_signal_id() -> String {
    ulid::Ulid::from_datetime(std::time::SystemTime::now()).to_string()
}

/// Offer what was heard to every run holding for it and to every listener
/// that hears it. Returns the signals written for listeners.
pub fn offer(inner: &Arc<Inner>, heard: &Heard, dedupe: Option<&str>) -> Vec<String> {
    crate::waits::on_heard(inner, heard);
    to_listeners(inner, heard, dedupe)
}

/// The listeners' half of [`offer`].
pub(crate) fn to_listeners(inner: &Arc<Inner>, heard: &Heard, dedupe: Option<&str>) -> Vec<String> {
    if !inner.listen.settings().enabled {
        return Vec::new();
    }
    let registry = armed(inner);
    registry
        .armed
        .iter()
        .filter(|a| a.hears(heard))
        .filter_map(|a| enqueue_for(inner, a, heard, dedupe, None))
        .collect()
}

/// Write one occurrence down for one listener: refused by the causal chain,
/// held to the rate ceiling, bounded by the backlog, and — `held` — kept for
/// the content screen before anything can start from it. Returns the signal.
pub(crate) fn enqueue_for(
    inner: &Arc<Inner>,
    armed: &Armed,
    heard: &Heard,
    dedupe: Option<&str>,
    held: Option<&str>,
) -> Option<String> {
    let settings = inner.listen.settings();
    if let Some(refusal) = heard.chain.refuses(&armed.key, settings.chain_cap()) {
        // The loop guard doing its job: worth a line, not an alarm.
        tracing::debug!(target: "bisa_engine::listen", listener = %armed.key, ?refusal, "the causal chain refused an occurrence");
        return None;
    }
    let now = now_secs();
    let mut state = SignalState::Queued;
    let mut note: Option<String> = None;
    if armed.on.rate_limited() && !inner.listen.allow_fire(&armed.key, now) {
        if matches!(armed.on, StartOn::Signal { .. }) {
            // A signal's overflow waits in the backlog for the window.
            state = SignalState::Waiting;
            note = Some(format!(
                "{}: {} a minute; it waits for the window",
                super::OVER_RATE,
                settings.fires_per_minute
            ));
        } else {
            report_once(
                inner,
                &armed.key,
                None,
                format!(
                    "over {} occurrences a minute; skipped until the window clears",
                    settings.fires_per_minute
                ),
            );
            return None;
        }
    }
    let pending = inner
        .ws
        .pending_signals(&armed.key)
        .map(|p| p.len())
        .unwrap_or(0);
    if pending >= settings.backlog() {
        report_once(
            inner,
            &armed.key,
            None,
            format!("its backlog is full ({pending} waiting); an occurrence was dropped"),
        );
        return None;
    }
    if let Some(why) = held {
        state = SignalState::Held;
        note = Some(why.to_string());
    }
    let signal = Signal {
        id: new_signal_id(),
        listener: Some(armed.key.clone()),
        source: heard.source,
        name: heard.name.clone(),
        at: now,
        payload: heard.payload.clone(),
        scope: armed.scope(),
        chain: heard.chain.extend(&armed.key),
        dedupe_key: dedupe.map(str::to_string),
    };
    match inner.ws.enqueue_signal_as(&signal, state, note.as_deref()) {
        Ok((queued, fresh)) => {
            if fresh {
                announce_received(inner, &queued.signal);
                if queued.state == SignalState::Queued {
                    inner.listen.wake();
                }
            }
            Some(queued.signal.id)
        }
        // LCOV_EXCL_START: writing an occurrence down fails only with the index unwritable (disk-only)
        Err(e) => {
            report_once(
                inner,
                &armed.key,
                None,
                format!("an occurrence could not be written down: {e}"),
            );
            None
            // LCOV_EXCL_STOP
        }
    }
}

/// `SignalReceived`, after the write — never before it.
pub(crate) fn announce_received(inner: &Inner, signal: &Signal) {
    let payload = EnginePayload::SignalReceived {
        signal: signal.id.clone(),
        listener: signal.listener.clone(),
        source: signal.source,
    };
    inner.emit(match &signal.listener {
        Some(key) => super::host_event(inner, key, payload),
        None => match signal.scope.goal() {
            Some(goal) => EngineEvent::scoped(goal, None, payload),
            None => EngineEvent::global(payload),
        },
    });
}

/// Subscribe to the engine's bus and hear every event once.
pub fn spawn(inner: &Arc<Inner>) -> tokio::task::JoinHandle<()> {
    // Subscribe before spawning: a receiver made inside the task would miss
    // everything emitted while it was being scheduled.
    let mut rx = inner.subscribe();
    let inner = Arc::clone(inner);
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                // Contained per event: this task is the one ear of every
                // listener, wait and boundary, and a panic on one event must
                // never deafen them all until the next process.
                Ok(event) => crate::survive("event ear", async { on_event(&inner, &event) }).await,
                // LCOV_EXCL_START: the ear's own bus lagging or closing: a race with the engine's end no test can stage
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(target: "bisa_engine::listen", "the event ear lagged by {n} events");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    })
    // LCOV_EXCL_STOP
}

/// One bus event: the registry told when what it is built from moved, the
/// loop guard told what a run's work made, and the event heard — as a run's
/// end, and as its topic. Public so a test can hear an event without racing
/// a live bus.
pub fn on_event(inner: &Arc<Inner>, event: &EngineEvent) {
    keep_registry(inner, event);
    keep_loop_guard(inner, event);
    let topic = event.payload.topic();
    if OWN_TOPICS.contains(&topic) {
        return;
    }
    if let Some(heard) = run_end(event) {
        let dedupe = match &event.payload {
            EnginePayload::RunFinished { run, .. } | EnginePayload::RunCancelled { run, .. } => {
                Some(format!("run:{run}"))
            }
            // LCOV_EXCL_START: a dedupe key is minted for a run's end alone; the arm keeps the match total
            _ => None,
            // LCOV_EXCL_STOP
        };
        hear(inner, heard, event.run, dedupe.as_deref());
    }
    let platform = Heard {
        source: SignalSource::Platform,
        name: Some(topic.to_string()),
        scope: scope_of(event),
        payload: json!({
            "event": topic,
            "fields": event.payload.fields(),
            "goal": event.goal.map(|g| g.to_string()),
            "workflow": event.workflow.map(|w| w.to_string()),
            "run": event.run.map(|r| r.to_string()),
        }),
        chain: bisa_core::Chain::default(),
    };
    hear(inner, platform, event.run, None);
}

/// Offer `heard` when anyone hears it — the chain behind it read only then,
/// from the run the event was about.
fn hear(inner: &Arc<Inner>, mut heard: Heard, run: Option<RunId>, dedupe: Option<&str>) {
    let listeners =
        inner.listen.settings().enabled && armed(inner).armed.iter().any(|a| a.hears(&heard));
    let holders = crate::waits::hears(inner, &heard);
    if !listeners && !holders {
        return;
    }
    if let Some(run) = run {
        heard.chain = chain_of_run(inner, run);
    }
    offer(inner, &heard, dedupe);
}

/// Where a bus event belongs: its goal's, or nobody's in particular.
fn scope_of(event: &EngineEvent) -> SignalScope {
    match event.goal {
        Some(goal) => SignalScope::Goal { goal },
        None => SignalScope::Workspace,
    }
}

/// A run that ended, as a `run` start, wait or boundary hears it:
/// `{ run, workflow, outcome, goal? }`.
fn run_end(event: &EngineEvent) -> Option<Heard> {
    let (run, workflow, outcome) = match &event.payload {
        EnginePayload::RunFinished {
            run,
            workflow,
            outcome,
        } => (
            run,
            workflow,
            match outcome {
                RunOutcome::Done => "done",
                RunOutcome::Failed => "failed",
            },
        ),
        EnginePayload::RunCancelled {
            run,
            workflow,
            cause,
        } => {
            // A restart is not an end: the run goes on as its replacement.
            if matches!(cause, CancelCause::Restarted) {
                return None;
            }
            (run, workflow, "cancelled")
        }
        _ => return None,
    };
    Some(Heard {
        source: SignalSource::Run,
        name: None,
        scope: scope_of(event),
        payload: json!({
            "run": run.to_string(),
            "workflow": workflow.to_string(),
            "outcome": outcome,
            "goal": event.goal.map(|g| g.to_string()),
        }),
        chain: bisa_core::Chain::default(),
    })
}

/// Drop the registry when what it is built from may have moved: a host
/// turned on or off, a definition, a goal, a connector, a project, the people
/// and staff an input may name, the `events.*` settings.
fn keep_registry(inner: &Inner, event: &EngineEvent) {
    let moves = match &event.payload {
        EnginePayload::ListeningChanged { .. }
        | EnginePayload::WorkflowChanged { .. }
        | EnginePayload::WorkflowProposed { .. }
        | EnginePayload::WorkflowDeleted { .. }
        | EnginePayload::WorkflowArchived { .. }
        | EnginePayload::GoalClosed { .. }
        | EnginePayload::GoalArchived { .. }
        | EnginePayload::GoalDeleted { .. }
        | EnginePayload::ConnectorsChanged { .. }
        | EnginePayload::ProjectArchived { .. }
        | EnginePayload::ProjectDeleted { .. }
        | EnginePayload::PeopleChanged { .. } => true,
        EnginePayload::SettingsChanged { keys, .. } => {
            keys.iter().any(|k| k.starts_with("events."))
        }
        _ => false,
    };
    if moves {
        super::invalidate(inner);
    }
}

/// What a run's work made, remembered so the event it makes later carries the
/// run's chain; a workstream that moved asks pull request states sooner.
fn keep_loop_guard(inner: &Inner, event: &EngineEvent) {
    match &event.payload {
        EnginePayload::WorkstreamCommitted { commit, .. } => {
            if let Some(run) = event.run {
                super::note_commit_run(inner, commit, run);
            }
        }
        EnginePayload::WorkstreamChanged { .. } => {
            inner
                .listen
                .prs_due
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }
        _ => {}
    }
}

/// A message posted into a conversation, heard once — where dispatch hears
/// it, after the hold. An announcement is never heard: a workflow's own post
/// is the platform speaking, not somebody saying something.
///
/// The payload: `{ message, scope, author, author_kind: you | agent | person,
/// teams, mentions, text }` — `author` an agent's id or a person's key, the
/// mentions likewise.
pub(crate) fn on_message(
    inner: &Arc<Inner>,
    scope: &str,
    event: &nostr::event::Event,
    origin: PostOrigin,
) {
    if origin == PostOrigin::Announced {
        return;
    }
    let listeners = inner.listen.settings().enabled && armed(inner).wants(SignalSource::Message);
    if !listeners && !crate::waits::wants(inner, SignalSource::Message) {
        return;
    }
    let author_hex = event.pubkey.to_hex();
    let agents = inner.ws.list_agents().unwrap_or_default();
    let author_agent = agents.iter().find(|a| a.pubkey.as_hex() == author_hex);
    let (author, author_kind, who) = match author_agent {
        Some(agent) => (
            agent.id.to_string(),
            "agent",
            Assignee::Agent(agent.id.to_string()),
        ),
        None => {
            let kind = if inner.ws.owner_principal().as_hex() == author_hex {
                "you"
            } else {
                // LCOV_EXCL_START: a person who is not you writes through collaboration (Phase 14); every message here is yours or an agent's
                "person"
                // LCOV_EXCL_STOP
            };
            match bisa_core::PrincipalId::new(author_hex.clone()) {
                Ok(pk) => (author_hex.clone(), kind, Assignee::Human(pk)),
                // LCOV_EXCL_START: an author that is no public key never reaches the ear: the store refuses the message
                Err(_) => return,
                // LCOV_EXCL_STOP
            }
        }
    };
    let teams: Vec<String> = inner
        .ws
        .list_teams()
        .unwrap_or_default()
        .into_iter()
        .filter(|t| t.members.contains(&who))
        .map(|t| t.id.to_string())
        .collect();
    let mentions: Vec<String> = event
        .tags
        .iter()
        .filter_map(|t| {
            let s = t.as_slice();
            (s.len() >= 2 && s[0] == "p").then(|| s[1].to_string())
        })
        .map(|pk| {
            agents
                .iter()
                .find(|a| a.pubkey.as_hex() == pk)
                .map(|a| a.id.to_string())
                .unwrap_or(pk)
        })
        .collect();
    let text = serde_json::from_str::<MessageBody>(&event.content)
        .ok()
        .and_then(|b| match b {
            MessageBody::Post { text, .. } => Some(text),
            // LCOV_EXCL_START: a message body that is no post never reaches the ear: the store refuses the message
            _ => None,
            // LCOV_EXCL_STOP
        })
        .unwrap_or_default();
    let message = event.id.to_hex();
    let facts = crate::conversation::scope_facts(inner, scope);
    let chain = super::run_of_message(inner, &message)
        .map(|run| chain_of_run(inner, run))
        .unwrap_or_default();
    let heard = Heard {
        source: SignalSource::Message,
        name: None,
        scope: match facts.goal {
            Some(goal) => SignalScope::Goal { goal },
            None => SignalScope::Workspace,
        },
        payload: json!({
            "message": message,
            "scope": scope,
            "author": author,
            "author_kind": author_kind,
            "teams": teams,
            "mentions": mentions,
            "text": text,
        }),
        chain,
    };
    offer(inner, &heard, Some(&format!("message:{message}")));
}

/// A listener's occurrence the ticker or a door produced itself — a
/// schedule, a poll, a check, a hook, a project change — written for it.
pub(crate) fn occur(
    inner: &Arc<Inner>,
    armed: &Armed,
    payload: Value,
    dedupe: &str,
    held: Option<&str>,
) -> Option<String> {
    let heard = Heard {
        source: armed.source(),
        name: None,
        scope: armed.scope(),
        payload,
        chain: bisa_core::Chain::default(),
    };
    enqueue_for(inner, armed, &heard, Some(dedupe), held)
}

/// Which listener a signal is for — for a caller that has only the key.
pub(crate) fn armed_for(inner: &Arc<Inner>, key: &ListenerKey) -> Option<Armed> {
    armed(inner).get(key).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bisa_core::{GoalId, WorkflowId};

    fn run() -> RunId {
        RunId::from_ulid(ulid::Ulid::from_parts(1, 1))
    }

    fn wf() -> WorkflowId {
        WorkflowId::from_ulid(ulid::Ulid::from_parts(2, 2))
    }

    #[test]
    fn a_run_that_ended_is_heard_with_its_outcome_and_a_restart_is_not() {
        let goal = GoalId::from_ulid(ulid::Ulid::from_parts(3, 3));
        let failed = EngineEvent::scoped(
            goal,
            None,
            EnginePayload::RunFinished {
                run: run(),
                workflow: wf(),
                outcome: RunOutcome::Failed,
            },
        );
        let heard = run_end(&failed).expect("a run's end");
        assert_eq!(heard.source, SignalSource::Run);
        assert_eq!(heard.scope, SignalScope::Goal { goal });
        assert_eq!(heard.payload["outcome"], "failed");
        assert_eq!(heard.payload["workflow"], wf().to_string());
        let restarted = EngineEvent::global(EnginePayload::RunCancelled {
            run: run(),
            workflow: wf(),
            cause: CancelCause::Restarted,
        });
        assert!(run_end(&restarted).is_none());
        let stopped = EngineEvent::global(EnginePayload::RunCancelled {
            run: run(),
            workflow: wf(),
            cause: CancelCause::Stopped { rationale: None },
        });
        assert_eq!(run_end(&stopped).unwrap().payload["outcome"], "cancelled");
        assert!(run_end(&EngineEvent::global(EnginePayload::Paused)).is_none());
    }

    #[test]
    fn the_runtimes_own_news_is_never_heard_back() {
        for topic in OWN_TOPICS {
            assert!(crate::events::TOPICS.contains(topic), "{topic}");
        }
        assert!(!OWN_TOPICS.contains(&"run.finished"));
    }
}
