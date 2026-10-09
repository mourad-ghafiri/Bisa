//! The durable signal queue.
//!
//! An occurrence is **written down before anything acts on it**: a signal for
//! the listener it is for, carrying a `dedupe_key` its source derives, so a
//! crash between hearing and starting — or a hook delivered twice — makes one
//! signal and, at dispatch, one run. Truth is `events/queue.jsonl`, an
//! append-only log of enqueues and state changes (last line per id wins); the
//! `signals` index table is the rebuildable working set the worker claims
//! from.
//!
//! `queued → running → done | skipped | failed`, with two holds on the way:
//! `waiting` — the listener's guard keeps it behind a run still live, and
//! [`Workspace::release_signal`] puts it back — and `held`, an outside payload
//! the content screen would not pass, waiting on a person. A named signal an
//! `emit` raises is also kept once with **no listener**, settled at once, so a
//! `wait` re-armed after a restart can replay what it missed; the worker
//! never claims one.
//!
//! The journal fact (kind 3410) is written where the signal starts a run —
//! on that run's home ([`Workspace::journal_signal`]) — never at enqueue: an
//! occurrence nobody's run came of is the queue's, not a goal's story.

use crate::error::StoreError;
use crate::index::SignalRow;
use crate::workspace::{now_secs, Workspace};
use bisa_core::event::JournalPayload;
use bisa_core::signal::MAX_SIGNAL_PAYLOAD_BYTES;
use bisa_core::{
    Chain, GoalId, Home, ListenerHost, ListenerKey, Signal, SignalScope, SignalSource, StepId,
};
use serde::{Deserialize, Serialize};

/// One line of `events/queue.jsonl`. A `State` line carries the attempt count
/// and the last note with it, so a rebuild restores the working set whole —
/// not just which signals settled, but how hard each was and why.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "op")]
enum QueueLine {
    Enqueued {
        signal: Box<Signal>,
    },
    State {
        id: String,
        state: String,
        #[serde(default)]
        attempts: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        last_error: Option<String>,
    },
}

/// Where a signal stands in the queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignalState {
    Queued,
    Running,
    /// Kept behind a run of its listener that is still live.
    Waiting,
    /// Waiting on a person: an outside payload the screen would not pass.
    Held,
    /// It began a run.
    Done,
    /// Dropped — by the guard, a chain, a listener no longer on — and why.
    Skipped,
    Failed,
}

impl SignalState {
    pub fn as_str(self) -> &'static str {
        match self {
            SignalState::Queued => "queued",
            SignalState::Running => "running",
            SignalState::Waiting => "waiting",
            SignalState::Held => "held",
            SignalState::Done => "done",
            SignalState::Skipped => "skipped",
            SignalState::Failed => "failed",
        }
    }

    pub fn parse(word: &str) -> Option<Self> {
        Some(match word {
            "queued" => SignalState::Queued,
            "running" => SignalState::Running,
            "waiting" => SignalState::Waiting,
            "held" => SignalState::Held,
            "done" => SignalState::Done,
            "skipped" => SignalState::Skipped,
            "failed" => SignalState::Failed,
            _ => return None,
        })
    }

    /// Nothing moves it any more.
    pub fn is_settled(self) -> bool {
        matches!(
            self,
            SignalState::Done | SignalState::Skipped | SignalState::Failed
        )
    }
}

/// A signal as the queue holds it: the occurrence, and where it stands.
#[derive(Clone, Debug, PartialEq)]
pub struct QueuedSignal {
    pub signal: Signal,
    pub state: SignalState,
    pub attempts: u32,
    /// Why it was skipped, failed or is held.
    pub note: Option<String>,
}

/// The index row for a signal about to be written.
fn row_of(signal: &Signal, state: SignalState) -> Result<SignalRow, StoreError> {
    let (host_kind, host_id, step) = match &signal.listener {
        Some(key) => (
            Some(key.host.kind().to_string()),
            Some(key.host.id()),
            Some(key.step.to_string()),
        ),
        None => (None, None, None),
    };
    let (scope_kind, scope_id) = match &signal.scope {
        SignalScope::Workspace => ("workspace", None),
        SignalScope::Goal { goal } => ("goal", Some(goal.to_string())),
    };
    Ok(SignalRow {
        id: signal.id.clone(),
        host_kind,
        host_id,
        step,
        source: signal.source.as_str().to_string(),
        name: signal.name.clone(),
        scope_kind: scope_kind.to_string(),
        scope_id,
        dedupe_key: signal.dedupe_key.clone(),
        state: state.as_str().to_string(),
        at: signal.at,
        attempts: 0,
        payload_json: serde_json::to_string(&signal.payload)?,
        chain_json: serde_json::to_string(&signal.chain)?,
        last_error: None,
    })
}

/// The signal back from its row. A row the index cannot read whole is a
/// broken cache: what can be read is kept, the rest defaults.
fn queued_of(row: &SignalRow) -> QueuedSignal {
    let listener = match (&row.host_kind, &row.host_id, &row.step) {
        (Some(kind), Some(id), Some(step)) => ListenerHost::from_parts(kind, id)
            .zip(StepId::new(step.as_str()).ok())
            .map(|(host, step)| ListenerKey { host, step }),
        _ => None,
    };
    let scope = match (row.scope_kind.as_str(), &row.scope_id) {
        ("goal", Some(id)) => match id.parse::<GoalId>() {
            Ok(goal) => SignalScope::Goal { goal },
            Err(_) => SignalScope::Workspace,
        },
        _ => SignalScope::Workspace,
    };
    QueuedSignal {
        signal: Signal {
            id: row.id.clone(),
            listener,
            source: SignalSource::parse(&row.source).unwrap_or(SignalSource::Signal),
            name: row.name.clone(),
            at: row.at,
            payload: serde_json::from_str(&row.payload_json).unwrap_or(serde_json::Value::Null),
            scope,
            chain: serde_json::from_str::<Chain>(&row.chain_json).unwrap_or_default(),
            dedupe_key: row.dedupe_key.clone(),
        },
        state: SignalState::parse(&row.state).unwrap_or(SignalState::Failed),
        attempts: row.attempts,
        note: row.last_error.clone(),
    }
}

impl Workspace {
    fn append_queue_line(&self, line: &QueueLine) -> Result<(), StoreError> {
        crate::paths::append_line(&self.paths.signal_queue(), &serde_json::to_string(line)?)
    }

    /// Write one occurrence down — for its listener, queued for dispatch; or,
    /// a named signal with no listener, settled at once for waits to replay.
    /// Idempotent on the id and on `(listener, dedupe_key)`: a second write
    /// returns the signal already held and `false`. An oversized payload is
    /// refused outright rather than truncated.
    pub fn enqueue_signal(&self, signal: &Signal) -> Result<(QueuedSignal, bool), StoreError> {
        let state = if signal.listener.is_some() {
            SignalState::Queued
        } else {
            SignalState::Done
        };
        self.enqueue_signal_as(signal, state, None)
    }

    /// [`Self::enqueue_signal`] in a state other than the one it would take:
    /// `waiting` — over its listener's rate, until the window clears — or
    /// `held` — an outside payload the content screen reads first — with the
    /// note that says why. A listener-less signal is always settled at once.
    pub fn enqueue_signal_as(
        &self,
        signal: &Signal,
        state: SignalState,
        note: Option<&str>,
    ) -> Result<(QueuedSignal, bool), StoreError> {
        let payload_json = serde_json::to_string(&signal.payload)?;
        if payload_json.len() > MAX_SIGNAL_PAYLOAD_BYTES {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-signal-payload-bytes-over-byte-cap",
                a0 = (payload_json.len()).to_string(),
                max_signal_payload_bytes = (MAX_SIGNAL_PAYLOAD_BYTES).to_string()
            )));
        }
        if signal.id.trim().is_empty() {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-signal-needs-id"
            )));
        }
        if let Some(held) = self.signal_already_held(signal)? {
            return Ok((held, false)); // a replay, not a second copy
        }
        let state = if signal.listener.is_some() {
            state
        } else {
            SignalState::Done
        };
        // Truth first, then the index: a crash between the two loses nothing.
        self.append_queue_line(&QueueLine::Enqueued {
            signal: Box::new(signal.clone()),
        })?;
        if state != SignalState::Queued {
            self.append_queue_line(&QueueLine::State {
                id: signal.id.clone(),
                state: state.as_str().into(),
                attempts: 0,
                last_error: note.map(str::to_string),
            })?;
        }
        let mut row = row_of(signal, state)?;
        row.last_error = note.map(str::to_string);
        self.idx().insert_signal(&row)?;
        Ok((queued_of(&row), true))
    }

    /// The signal this workspace already holds for the same occurrence: the
    /// same id, or the same listener and dedupe key.
    fn signal_already_held(&self, signal: &Signal) -> Result<Option<QueuedSignal>, StoreError> {
        let idx = self.idx();
        if let Some(row) = idx.get_signal(&signal.id)? {
            return Ok(Some(queued_of(&row)));
        }
        let (Some(key), Some(dedupe)) = (&signal.listener, &signal.dedupe_key) else {
            return Ok(None);
        };
        Ok(idx
            .signal_by_dedupe(key.host.kind(), &key.host.id(), key.step.as_str(), dedupe)?
            .map(|row| queued_of(&row)))
    }

    /// Take the oldest ready signal, marking it `running` atomically. The
    /// claim is the index's (one `UPDATE … RETURNING`, so two workers never
    /// take one signal) and is written to the truth log right after: a crash
    /// between leaves a signal the log still says is queued, which the index
    /// rebuilds as queued — at-least-once, never lost; the run it would start
    /// is refused a second time by its `dispatched` id.
    pub fn claim_next_signal(&self) -> Result<Option<QueuedSignal>, StoreError> {
        let row = self.idx().claim_next_signal(now_secs())?;
        let Some(row) = row else { return Ok(None) };
        self.append_queue_line(&QueueLine::State {
            id: row.id.clone(),
            state: SignalState::Running.as_str().into(),
            attempts: row.attempts,
            last_error: row.last_error.clone(),
        })?;
        Ok(Some(queued_of(&row)))
    }

    /// Move a signal: settle it, hold it, or put it back. Truth first, then
    /// the index — a crash between the two leaves a log the next rebuild
    /// reads as the move made.
    pub fn move_signal(
        &self,
        id: &str,
        state: SignalState,
        note: Option<&str>,
    ) -> Result<(), StoreError> {
        let attempts = self
            .idx()
            .get_signal(id)?
            .map(|r| r.attempts)
            .ok_or_else(|| {
                StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-signal-not-found",
                    id = id.to_string()
                ))
            })?;
        self.append_queue_line(&QueueLine::State {
            id: id.to_string(),
            state: state.as_str().into(),
            attempts,
            last_error: note.map(str::to_string),
        })?;
        match state {
            SignalState::Queued => self.idx().requeue_signal(id),
            _ => self
                .idx()
                .set_signal_state(id, state.as_str(), None, attempts, note),
        }
    }

    /// A held signal may start now — its listener's run ended, or a person
    /// let an outside payload through.
    pub fn release_signal(&self, id: &str) -> Result<(), StoreError> {
        self.move_signal(id, SignalState::Queued, None)
    }

    /// Crash recovery: return `running` signals claimed more than
    /// `older_than_secs` ago to the queue — the truth log first, one line
    /// per signal, then the index. Returns the ids requeued.
    pub fn requeue_stale_running(&self, older_than_secs: u64) -> Result<Vec<String>, StoreError> {
        let cutoff = now_secs().saturating_sub(older_than_secs);
        let stale = self.idx().stale_running_signals(cutoff)?;
        let mut ids = Vec::with_capacity(stale.len());
        for row in stale {
            self.append_queue_line(&QueueLine::State {
                id: row.id.clone(),
                state: SignalState::Queued.as_str().into(),
                attempts: row.attempts,
                last_error: row.last_error.clone(),
            })?;
            self.idx().requeue_signal(&row.id)?;
            ids.push(row.id);
        }
        Ok(ids)
    }

    /// The newest signals of one host, or of every one.
    pub fn list_signals(
        &self,
        host: Option<&ListenerHost>,
        limit: usize,
    ) -> Result<Vec<QueuedSignal>, StoreError> {
        let host_id = host.map(ListenerHost::id);
        let rows = self.idx().list_signals(
            host.zip(host_id.as_deref())
                .map(|(host, id)| (host.kind(), id)),
            limit,
        )?;
        Ok(rows.iter().map(queued_of).collect())
    }

    /// A listener's backlog: its signals not settled yet, oldest first.
    pub fn pending_signals(&self, key: &ListenerKey) -> Result<Vec<QueuedSignal>, StoreError> {
        let rows = self.idx().pending_signals_of_listener(
            key.host.kind(),
            &key.host.id(),
            key.step.as_str(),
        )?;
        Ok(rows.iter().map(queued_of).collect())
    }

    /// When a listener last heard its event, if it ever did.
    pub fn last_signal_at(&self, key: &ListenerKey) -> Result<Option<u64>, StoreError> {
        self.idx()
            .last_signal_at(key.host.kind(), &key.host.id(), key.step.as_str())
    }

    /// Named signals raised at or after `since`, oldest first — what a `wait`
    /// re-armed after a restart hears again.
    pub fn named_signals_since(
        &self,
        name: &str,
        since: u64,
        limit: usize,
    ) -> Result<Vec<Signal>, StoreError> {
        let rows = self.idx().named_signals_since(name, since, limit)?;
        Ok(rows.iter().map(|r| queued_of(r).signal).collect())
    }

    /// Settle every signal of a host that has not settled — it stopped
    /// listening, or a failed run paused it — saying why. Returns the ids.
    pub fn settle_pending_signals(
        &self,
        host: &ListenerHost,
        state: SignalState,
        note: &str,
    ) -> Result<Vec<String>, StoreError> {
        let pending = self
            .idx()
            .pending_signals_of_host(host.kind(), &host.id())?;
        let mut ids = Vec::with_capacity(pending.len());
        for row in pending {
            self.move_signal(&row.id, state, Some(note))?;
            ids.push(row.id);
        }
        Ok(ids)
    }

    /// Forget a host's signals from the index — the host is gone. The log
    /// keeps them as history; a rebuild skips them.
    pub(crate) fn forget_signals_of(&self, host: &ListenerHost) -> Result<(), StoreError> {
        self.idx().delete_signals_of_host(host.kind(), &host.id())
    }

    pub fn signal(&self, id: &str) -> Result<Option<QueuedSignal>, StoreError> {
        Ok(self.idx().get_signal(id)?.map(|r| queued_of(&r)))
    }

    pub fn signal_state(&self, id: &str) -> Result<Option<SignalState>, StoreError> {
        Ok(self.signal(id)?.map(|q| q.state))
    }

    /// The fact of the occurrence that began a run (kind 3410), on the run's
    /// home — its goal's journal, or the run's own.
    pub fn journal_signal(&self, home: &Home, signal: &Signal) -> Result<(), StoreError> {
        let owner = self.owner.clone();
        self.append_journal(
            home,
            JournalPayload::Signal {
                signal: signal.id.clone(),
                listener: signal.listener.clone(),
                source: signal.source,
                name: signal.name.clone(),
                payload: signal.payload.clone(),
            },
            &owner,
            None,
        )
        .map(|_| ())
    }

    /// Rebuild support: replay `events/queue.jsonl` into the `signals` table.
    /// A signal whose host is gone — a workflow or a goal deleted — is history
    /// the working set does not hold, and is skipped; a named signal kept for
    /// replay has no host and always comes back.
    pub(crate) fn reindex_signals(&self) -> Result<(), StoreError> {
        let path = self.paths.signal_queue();
        // Read lossily: a torn multi-byte character costs its line, never
        // the queue or the rebuild.
        let content = match std::fs::read(&path) {
            Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(StoreError::io(path.display().to_string(), e)),
        };
        let mut states: Vec<(String, String, u32, Option<String>)> = Vec::new();
        for (n, line) in content
            .lines()
            .enumerate()
            .filter(|(_, l)| !l.trim().is_empty())
        {
            match serde_json::from_str::<QueueLine>(line) {
                Ok(QueueLine::Enqueued { signal }) => {
                    if let Some(key) = &signal.listener {
                        if !self.host_exists(&key.host)? {
                            continue;
                        }
                    }
                    self.idx()
                        .insert_signal(&row_of(&signal, SignalState::Queued)?)?;
                }
                Ok(QueueLine::State {
                    id,
                    state,
                    attempts,
                    last_error,
                }) => states.push((id, state, attempts, last_error)),
                Err(e) => tracing::warn!(
                    target: "bisa_store::signals",
                    // LCOV_EXCL_START: a tracing line's fields are counted on the macro's own line; the line they make is read back by the crate's tests
                    path = %path.display(),
                    line = n + 1,
                    // LCOV_EXCL_STOP
                    "a line of the signal queue does not parse and is skipped; what it held is not replayed: {e}"
                ),
            }
        }
        let now = now_secs();
        let idx = self.idx();
        for (id, state, attempts, last_error) in states {
            if SignalState::parse(&state).is_none() {
                continue;
            }
            // A row replayed as `running` gets a fresh `started_at`: recovery
            // waits out the full threshold instead of yanking a signal a live
            // worker may still hold.
            let started_at = (state == SignalState::Running.as_str()).then_some(now);
            idx.set_signal_state(&id, &state, started_at, attempts, last_error.as_deref())?;
        }
        Ok(())
    }

    /// Whether a host is still in the workspace: its workflow, or its goal.
    pub(crate) fn host_exists(&self, host: &ListenerHost) -> Result<bool, StoreError> {
        let idx = self.idx();
        match host {
            ListenerHost::Workspace { workflow } => {
                Ok(idx.get_workflow(&workflow.to_string())?.is_some())
            }
            ListenerHost::Goal { goal } => idx.goal_exists(&goal.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::MemoryKeyStore;
    use bisa_core::WorkflowId;

    fn ws() -> (tempfile::TempDir, Workspace, WorkflowId) {
        let dir = tempfile::tempdir().unwrap();
        let w =
            Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
        let wf = w
            .create_workflow(
                crate::workflows::tests::notify_workflow("Post"),
                bisa_core::WorkflowOrigin::Workspace,
            )
            .unwrap();
        (dir, w, wf.id)
    }

    fn key(wf: WorkflowId, step: &str) -> ListenerKey {
        ListenerKey {
            host: ListenerHost::Workspace { workflow: wf },
            step: StepId::new(step).unwrap(),
        }
    }

    fn signal(id: &str, listener: Option<ListenerKey>, at: u64, dedupe: Option<&str>) -> Signal {
        Signal {
            id: id.into(),
            listener,
            source: SignalSource::Schedule,
            name: None,
            at,
            payload: serde_json::json!({"n": 1}),
            scope: SignalScope::Workspace,
            chain: Chain::default(),
            dedupe_key: dedupe.map(str::to_string),
        }
    }

    /// Within one second the queue's order is the order of arrival. A signal
    /// a run raised is named after its dedupe key — a hash — so its id says
    /// nothing of when it came; two of them in one second were read in the
    /// order of their hashes, and a wait armed again would hear the later
    /// one first.
    #[test]
    fn signals_of_one_second_are_read_in_the_order_they_came() {
        let (_dir, ws, wf) = ws();
        let named = |id: &str| Signal {
            source: SignalSource::Signal,
            name: Some("pager.ring".into()),
            ..signal(id, None, 100, Some(id))
        };
        // The ids sort against the order of arrival.
        for id in ["emit-ff", "emit-88", "emit-00"] {
            ws.enqueue_signal(&named(id)).unwrap();
        }
        let ids = |signals: Vec<Signal>| signals.into_iter().map(|s| s.id).collect::<Vec<_>>();
        assert_eq!(
            ids(ws.named_signals_since("pager.ring", 0, 50).unwrap()),
            ["emit-ff", "emit-88", "emit-00"],
            "oldest first"
        );

        let k = key(wf, "nightly");
        for id in ["zz", "mm", "aa"] {
            ws.enqueue_signal(&signal(id, Some(k.clone()), 100, None))
                .unwrap();
        }
        let claimed: Vec<String> = std::iter::from_fn(|| ws.claim_next_signal().unwrap())
            .map(|q| q.signal.id)
            .collect();
        assert_eq!(claimed, ["zz", "mm", "aa"], "claimed as they came");
    }

    #[test]
    fn enqueue_claim_settle() {
        let (_dir, ws, wf) = ws();
        let k = key(wf, "nightly");
        ws.enqueue_signal(&signal("01A", Some(k.clone()), 100, None))
            .unwrap();
        ws.enqueue_signal(&signal("01B", Some(k.clone()), 200, None))
            .unwrap();
        assert_eq!(ws.signal_state("01A").unwrap(), Some(SignalState::Queued));
        let claimed = ws.claim_next_signal().unwrap().unwrap();
        assert_eq!(claimed.signal.id, "01A");
        assert_eq!(claimed.signal.listener, Some(k.clone()));
        assert_eq!(ws.signal_state("01A").unwrap(), Some(SignalState::Running));
        assert_eq!(ws.claim_next_signal().unwrap().unwrap().signal.id, "01B");
        assert!(ws.claim_next_signal().unwrap().is_none());
        ws.move_signal("01A", SignalState::Done, None).unwrap();
        ws.move_signal("01B", SignalState::Skipped, Some("busy"))
            .unwrap();
        assert_eq!(ws.signal_state("01A").unwrap(), Some(SignalState::Done));
        assert_eq!(
            ws.signal("01B").unwrap().unwrap().note.as_deref(),
            Some("busy")
        );
        assert!(ws.pending_signals(&k).unwrap().is_empty());
        assert_eq!(ws.last_signal_at(&k).unwrap(), Some(200));
    }

    #[test]
    fn an_occurrence_is_written_once_and_an_oversized_one_never() {
        let (_dir, ws, wf) = ws();
        let k = key(wf, "nightly");
        let a = signal("01A", Some(k.clone()), 100, Some("schedule:100"));
        assert!(ws.enqueue_signal(&a).unwrap().1);
        assert!(!ws.enqueue_signal(&a).unwrap().1, "the same id twice");
        let (held, new) = ws
            .enqueue_signal(&signal("01C", Some(k.clone()), 101, Some("schedule:100")))
            .unwrap();
        assert!(!new, "the same occurrence under another id");
        assert_eq!(held.signal.id, "01A");
        assert_eq!(ws.list_signals(None, 10).unwrap().len(), 1);
        let mut big = signal("01BIG", Some(k), 1, None);
        big.payload = serde_json::json!({ "blob": "x".repeat(MAX_SIGNAL_PAYLOAD_BYTES) });
        assert!(ws.enqueue_signal(&big).is_err());
    }

    #[test]
    fn a_named_signal_is_kept_for_replay_and_never_claimed() {
        let (_dir, ws, _wf) = ws();
        let mut emitted = signal("01E", None, 50, Some("emit:R:e:3"));
        emitted.source = SignalSource::Signal;
        emitted.name = Some("report.ready".into());
        let (kept, _) = ws.enqueue_signal(&emitted).unwrap();
        assert_eq!(kept.state, SignalState::Done);
        assert!(ws.claim_next_signal().unwrap().is_none());
        assert_eq!(
            ws.named_signals_since("report.ready", 40, 10).unwrap()[0].id,
            "01E"
        );
        assert!(ws
            .named_signals_since("report.ready", 60, 10)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn a_held_signal_waits_until_released_and_a_host_that_stops_settles_its_backlog() {
        let (_dir, ws, wf) = ws();
        let k = key(wf, "ticket");
        for (id, at) in [("01A", 100), ("01B", 200)] {
            ws.enqueue_signal(&signal(id, Some(k.clone()), at, None))
                .unwrap();
        }
        let first = ws.claim_next_signal().unwrap().unwrap();
        ws.move_signal(&first.signal.id, SignalState::Waiting, None)
            .unwrap();
        assert_eq!(
            ws.claim_next_signal().unwrap().unwrap().signal.id,
            "01B",
            "a signal its guard holds does not block the next"
        );
        assert!(ws.claim_next_signal().unwrap().is_none());
        assert_eq!(ws.pending_signals(&k).unwrap().len(), 1);
        ws.release_signal("01A").unwrap();
        assert_eq!(ws.claim_next_signal().unwrap().unwrap().signal.id, "01A");
        ws.move_signal("01A", SignalState::Held, Some("content"))
            .unwrap();
        let settled = ws
            .settle_pending_signals(&k.host, SignalState::Skipped, "not listening")
            .unwrap();
        assert_eq!(settled, vec!["01A"]);
        assert_eq!(ws.signal_state("01A").unwrap(), Some(SignalState::Skipped));
    }

    #[test]
    fn a_stale_running_signal_requeues_exactly_once() {
        let (_dir, ws, wf) = ws();
        ws.enqueue_signal(&signal("01A", Some(key(wf, "nightly")), 100, None))
            .unwrap();
        ws.claim_next_signal().unwrap().unwrap();
        assert!(ws.requeue_stale_running(3600).unwrap().is_empty());
        assert_eq!(ws.requeue_stale_running(0).unwrap(), vec!["01A"]);
        assert!(ws.requeue_stale_running(0).unwrap().is_empty());
        assert_eq!(ws.claim_next_signal().unwrap().unwrap().signal.id, "01A");
    }

    #[test]
    fn the_signal_that_began_a_run_reaches_that_runs_journal() {
        let (_dir, ws, wf) = ws();
        let goal = ws
            .create_goal(crate::workspace::NewGoal::captured("run the shop"))
            .unwrap();
        let mut s = signal("01A", Some(key(wf, "nightly")), 100, None);
        s.scope = SignalScope::Goal { goal: goal.id };
        ws.journal_signal(&Home::Goal { goal: goal.id }, &s)
            .unwrap();
        assert!(ws
            .journal(&Home::Goal { goal: goal.id })
            .unwrap()
            .iter()
            .any(|e| matches!(
                &e.payload,
                JournalPayload::Signal { signal, source: SignalSource::Schedule, .. } if signal == "01A"
            )));
    }

    #[test]
    fn the_queue_survives_a_rebuild_and_a_deleted_host_takes_its_signals() {
        let (_dir, ws, wf) = ws();
        let k = key(wf, "nightly");
        for (id, at) in [("01A", 100), ("01B", 200), ("01C", 300)] {
            ws.enqueue_signal(&signal(id, Some(k.clone()), at, None))
                .unwrap();
        }
        let mut emitted = signal("01E", None, 50, None);
        emitted.source = SignalSource::Signal;
        emitted.name = Some("done".into());
        ws.enqueue_signal(&emitted).unwrap();
        ws.claim_next_signal().unwrap();
        ws.claim_next_signal().unwrap();
        ws.move_signal("01B", SignalState::Done, None).unwrap();
        ws.rebuild_index().unwrap();
        assert_eq!(ws.signal_state("01A").unwrap(), Some(SignalState::Running));
        assert_eq!(ws.signal_state("01B").unwrap(), Some(SignalState::Done));
        assert_eq!(ws.signal_state("01C").unwrap(), Some(SignalState::Queued));
        assert_eq!(ws.signal_state("01E").unwrap(), Some(SignalState::Done));
        assert_eq!(ws.requeue_stale_running(0).unwrap(), vec!["01A"]);
        ws.delete_workflow(wf).unwrap();
        assert!(ws.list_signals(Some(&k.host), 10).unwrap().is_empty());
        ws.rebuild_index().unwrap();
        assert!(ws.list_signals(Some(&k.host), 10).unwrap().is_empty());
        assert_eq!(
            ws.signal_state("01E").unwrap(),
            Some(SignalState::Done),
            "a named signal names no host and outlives every one"
        );
    }

    /// A crash mid-write leaves a torn last line; a person's editor may
    /// leave worse. Whatever else the log holds is replayed, the bad line is
    /// skipped and said, and the rebuild does not fail.
    #[test]
    fn a_torn_or_corrupt_queue_line_is_skipped_and_the_rest_is_replayed() {
        let (_dir, ws, wf) = ws();
        let k = key(wf, "nightly");
        for (id, at) in [("01A", 100), ("01B", 200)] {
            ws.enqueue_signal(&signal(id, Some(k.clone()), at, None))
                .unwrap();
        }
        ws.claim_next_signal().unwrap();
        ws.move_signal("01A", SignalState::Done, None).unwrap();
        let path = ws.paths().signal_queue();
        let mut log = std::fs::read_to_string(&path).unwrap();
        log.push_str("{\"op\":\"enqueued\",\"signal\":{\"id\":\"01C\",\"lis");
        log.push_str("\nnot json at all\n");
        log.push_str("{\"op\":\"state\",\"id\":\"01Z\",\"state\":\"done\",\"attempts\":1}\n");
        std::fs::write(&path, log).unwrap();

        ws.rebuild_index().unwrap();
        assert_eq!(ws.signal_state("01A").unwrap(), Some(SignalState::Done));
        assert_eq!(ws.signal_state("01B").unwrap(), Some(SignalState::Queued));
        assert_eq!(ws.signal_state("01C").unwrap(), None);
        assert_eq!(ws.list_signals(None, 10).unwrap().len(), 2);
        assert_eq!(
            ws.claim_next_signal().unwrap().map(|s| s.signal.id),
            Some("01B".to_string())
        );
    }

    /// A line written by another shape of the code is skipped, never
    /// converted: its fields are not a signal's.
    #[test]
    fn a_queue_line_of_another_shape_is_skipped() {
        let (_dir, ws, _wf) = ws();
        let path = ws.paths().signal_queue();
        crate::paths::append_line(
            &path,
            r#"{"op":"enqueued","signal":{"id":"01OLD","trigger":"01T","topic":"cron.tick","at":1,"payload":{},"scope":{"scope":"workspace"}}}"#,
        )
        .unwrap();
        ws.rebuild_index().unwrap();
        assert_eq!(ws.signal_state("01OLD").unwrap(), None);
    }

    // added by the coverage pass: s3-signals.rs
    #[test]
    fn a_state_word_nobody_knows_is_none_a_settled_state_moves_no_more_and_an_unknown_signal_is_refused_by_name(
    ) {
        assert_eq!(SignalState::parse("bogus"), None);
        assert!(SignalState::Done.is_settled() && SignalState::Failed.is_settled());
        assert!(!SignalState::Queued.is_settled());
        let (_dir, ws, wf) = ws();
        assert!(matches!(
            ws.move_signal("nobody", SignalState::Done, None),
            Err(StoreError::Invalid(_))
        ));
        let mut nameless = signal("", Some(key(wf, "ticket")), 1, None);
        nameless.id = "  ".into();
        assert!(matches!(
            ws.enqueue_signal(&nameless),
            Err(StoreError::Invalid(_))
        ));
        // A queue line of a state nobody knows, and a goal-scoped row whose
        // goal id is no id, are passed over by the rebuild and the listing.
        let (queued, _) = ws
            .enqueue_signal(&signal("s1", Some(key(wf, "ticket")), 1, None))
            .unwrap();
        crate::paths::append_line(
            &ws.paths.signal_queue(),
            &serde_json::to_string(&QueueLine::State {
                id: queued.signal.id.clone(),
                state: "bogus".into(),
                attempts: 1,
                last_error: None,
            })
            .unwrap(),
        )
        .unwrap();
        ws.rebuild_index().unwrap();
        assert_eq!(
            ws.signal_state(&queued.signal.id).unwrap(),
            Some(SignalState::Queued)
        );
        ws.idx()
            .execute_for_test(&format!(
                "UPDATE signals SET scope_kind = 'goal', scope_id = 'not-a-goal' WHERE id = '{}'",
                queued.signal.id
            ))
            .unwrap();
        let listed = ws.list_signals(None, 10).unwrap();
        assert!(
            matches!(listed[0].signal.scope, SignalScope::Workspace),
            "{listed:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_queue_nobody_may_read_stops_the_rebuild_by_its_path() {
        use std::os::unix::fs::PermissionsExt;
        let (_dir, ws, wf) = ws();
        ws.enqueue_signal(&signal("s1", Some(key(wf, "ticket")), 1, None))
            .unwrap();
        let file = ws.paths.signal_queue();
        let was = std::fs::metadata(&file).unwrap().permissions();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).unwrap();
        let rebuilt = ws.rebuild_index();
        std::fs::set_permissions(&file, was).unwrap();
        assert!(matches!(rebuilt, Err(StoreError::Io { .. })), "{rebuilt:?}");
    }

    // added by the coverage pass: signals-s7.rs

    #[test]
    fn a_host_exists_while_its_goal_or_workflow_does() {
        let (_d, ws, _wf) = ws();
        let goal = ws
            .create_goal(crate::workspace::NewGoal::captured("hosted"))
            .unwrap();
        assert!(ws
            .host_exists(&bisa_core::ListenerHost::Goal { goal: goal.id })
            .unwrap());
        assert!(!ws
            .host_exists(&bisa_core::ListenerHost::Goal {
                goal: GoalId::from_ulid(crate::workspace::mint_ulid())
            })
            .unwrap());
    }
}
