//! Pending approval gates + the global pause gate.
//!
//! A gate is an in-memory wait point whose *resolution* is durable (a signed
//! Decision journal event via the store). A gate opened for a run step
//! carries the step, so the step's answer or decision lands on the run; a
//! gate with no step (a permission, an agent's own question, an adoption) is
//! decided in place.
//!
//! A gate carries `expects` — a plain approve/decline decision, or an answer
//! that may offer options. The resolution carries the answer back to the
//! waiting session, including the third outcome: "I'm not sure", which
//! resolves the gate without deciding it.

use crate::registry::LiveRunId;
use bisa_core::{Answer, AskKind, Gate};
use bisa_core::{GoalId, Home, RunId, StepId, WorkItemId};
use dashmap::{DashMap, DashSet};
use serde::Serialize;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::watch;

/// Resolution of a gate: the approve flag, what the human said (for
/// `AskKind::Answer` gates), and the journal event id of the signed decision
/// (the approval a `Decided` run event carries).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GateResolution {
    pub approve: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answer: Option<Answer>,
    pub decision_event_id: String,
    /// How many further clarify rounds the gate's home has left, present
    /// only when the human said they were not sure.
    ///
    /// Zero means the asker must now proceed on its own recommendation and
    /// journal the assumption: a question that can be re-asked forever is a
    /// way to never finish.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clarify_rounds_left: Option<u8>,
    /// The gate was taken back without a decision — the step it asked for
    /// was cancelled, the session that asked was stopped. A waiter reads it
    /// as a refusal that is nobody's: not remembered as the person's answer.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub withdrawn: bool,
}

impl GateResolution {
    /// The human resolved the gate without deciding it: they do not know.
    pub fn is_unsure(&self) -> bool {
        self.answer.as_ref().map(|a| a.unsure).unwrap_or(false)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct GateEntry {
    pub id: String,
    /// Where the question is asked: the goal it is about, or the run of the
    /// workspace — whose journal holds the decision and whose Inbox row
    /// (the run's workflow's) shows it.
    pub home: Home,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work_item: Option<WorkItemId>,
    /// The run step this gate completes, when it was opened for one: a
    /// `human` step's question, an `approval` step's decision.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run: Option<RunId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<StepId>,
    /// The session that asked, when a session did — a worker's or a design
    /// wake's permission, a question, a sign-in: what a stop of that session
    /// withdraws ([`Gates::pending_for_session`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<LiveRunId>,
    pub gate: Gate,
    /// What is being decided: `approval:<run>/<step>`, `step:<run>/<step>`,
    /// `adopt:<workflow>@<rev>`, `amend:<run>@<workflow>`, `permission:<tool>`,
    /// `ask_human:<scope>`.
    pub subject: String,
    /// Human-facing question for the inbox.
    pub question: String,
    /// What kind of human input this gate expects.
    pub expects: AskKind,
    pub opened_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution: Option<GateResolution>,
}

impl GateEntry {
    /// Whether this gate has been decided (approve or decline).
    ///
    /// An "I'm not sure" resolution is deliberately not a decision: it closes
    /// the wait so the asker can move, and says nothing about the question.
    pub fn decided(&self) -> Option<bool> {
        self.resolution
            .as_ref()
            .filter(|r| !r.is_unsure())
            .map(|r| r.approve)
    }
}

struct GateSlot {
    entry: GateEntry,
    tx: watch::Sender<Option<GateResolution>>,
}

#[derive(Default)]
pub struct Gates {
    slots: DashMap<String, Arc<GateSlot>>,
    /// The gates somebody is deciding right now ([`Gates::begin_decide`]).
    deciding: DashSet<String>,
}

/// Why a gate cannot be decided by this caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecideRefusal {
    /// No gate has this id.
    Unknown,
    /// It was decided already — or somebody is deciding it this instant,
    /// which is the same thing to whoever came second.
    Decided,
}

/// The one hand a gate is decided by. Held from the first check to the last
/// consequence of a decision, and released on drop: a decision that was
/// refused half way — an answer the gate does not offer, an input the
/// adoption lacks — leaves the gate open for the next try, and two
/// decisions arriving together are one decision and one refusal, never two
/// signed decisions and a consequence applied twice.
pub struct Deciding<'a> {
    gates: &'a Gates,
    id: String,
    pub entry: GateEntry,
}

impl Drop for Deciding<'_> {
    fn drop(&mut self) {
        self.gates.deciding.remove(&self.id);
    }
}

impl Gates {
    pub fn new() -> Self {
        Self::default()
    }

    /// Open a gate in `home`; returns its id and a receiver that resolves on
    /// decision.
    pub fn open(
        &self,
        home: Home,
        work_item: Option<WorkItemId>,
        gate: Gate,
        subject: impl Into<String>,
        question: impl Into<String>,
        expects: AskKind,
    ) -> (String, watch::Receiver<Option<GateResolution>>) {
        self.open_entry(
            home,
            work_item,
            None,
            None,
            gate,
            subject.into(),
            question.into(),
            expects,
        )
    }

    /// Open a gate a live session asks through — a permission, a question, a
    /// sign-in, a page to read — so a stop of that session can take the
    /// question back ([`Self::pending_for_session`]).
    #[allow(clippy::too_many_arguments)]
    pub fn open_for_session(
        &self,
        session: Option<LiveRunId>,
        home: Home,
        work_item: Option<WorkItemId>,
        gate: Gate,
        subject: impl Into<String>,
        question: impl Into<String>,
        expects: AskKind,
    ) -> (String, watch::Receiver<Option<GateResolution>>) {
        self.open_entry(
            home,
            work_item,
            session,
            None,
            gate,
            subject.into(),
            question.into(),
            expects,
        )
    }

    /// Open a gate that completes a run step. Deciding it lands a
    /// `RunEvent` on the run rather than resolving in place.
    #[allow(clippy::too_many_arguments)]
    pub fn open_for_step(
        &self,
        home: Home,
        run: RunId,
        step: StepId,
        gate: Gate,
        subject: impl Into<String>,
        question: impl Into<String>,
        expects: AskKind,
    ) -> (String, watch::Receiver<Option<GateResolution>>) {
        self.open_entry(
            home,
            None,
            None,
            Some((run, step)),
            gate,
            subject.into(),
            question.into(),
            expects,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn open_entry(
        &self,
        home: Home,
        work_item: Option<WorkItemId>,
        session: Option<LiveRunId>,
        step: Option<(RunId, StepId)>,
        gate: Gate,
        subject: String,
        question: String,
        expects: AskKind,
    ) -> (String, watch::Receiver<Option<GateResolution>>) {
        let id = ulid::Ulid::from_datetime(SystemTime::now()).to_string();
        let (run, step) = match step {
            Some((r, s)) => (Some(r), Some(s)),
            None => (None, None),
        };
        let entry = GateEntry {
            id: id.clone(),
            home,
            work_item,
            run,
            step,
            session,
            gate,
            subject,
            question,
            expects,
            opened_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            resolution: None,
        };
        let (tx, rx) = watch::channel(None);
        self.slots
            .insert(id.clone(), Arc::new(GateSlot { entry, tx }));
        (id, rx)
    }

    /// Pending gates, oldest first — the inbox.
    pub fn pending(&self) -> Vec<GateEntry> {
        let mut v: Vec<GateEntry> = self
            .slots
            .iter()
            .filter(|s| s.entry.resolution.is_none())
            .map(|s| s.entry.clone())
            .collect();
        v.sort_by_key(|e| e.opened_at);
        v
    }

    pub fn get(&self, id: &str) -> Option<GateEntry> {
        self.slots.get(id).map(|s| s.entry.clone())
    }

    /// Take the gate to decide it. See [`Deciding`].
    pub fn begin_decide(&self, id: &str) -> Result<Deciding<'_>, DecideRefusal> {
        if !self.slots.contains_key(id) {
            return Err(DecideRefusal::Unknown);
        }
        // The claim first, the look second: whoever inserts is alone with
        // the gate until its guard drops.
        if !self.deciding.insert(id.to_string()) {
            return Err(DecideRefusal::Decided);
        }
        let guard = |entry| Deciding {
            gates: self,
            id: id.to_string(),
            entry,
        };
        match self.get(id) {
            Some(entry) if entry.resolution.is_none() => Ok(guard(entry)),
            found => {
                self.deciding.remove(id);
                Err(if found.is_some() {
                    DecideRefusal::Decided
                } else {
                    DecideRefusal::Unknown
                })
            }
        }
    }

    /// The pending gates opened for one run step.
    pub fn pending_for_step(&self, run: RunId, step: &StepId) -> Vec<GateEntry> {
        self.pending()
            .into_iter()
            .filter(|g| g.run == Some(run) && g.step.as_ref() == Some(step))
            .collect()
    }

    /// The pending gates one live session asked through.
    pub fn pending_for_session(&self, session: LiveRunId) -> Vec<GateEntry> {
        self.pending()
            .into_iter()
            .filter(|g| g.session == Some(session))
            .collect()
    }

    /// The pending gates of one home — a goal, or a run of the workspace.
    pub fn pending_for_home(&self, home: &Home) -> Vec<GateEntry> {
        self.pending()
            .into_iter()
            .filter(|g| g.home == *home)
            .collect()
    }

    /// The pending gates of one goal.
    pub fn pending_for_goal(&self, goal: GoalId) -> Vec<GateEntry> {
        self.pending_for_home(&Home::Goal { goal })
    }

    /// Take a pending gate back without deciding it: the step it was asking
    /// for was cancelled or amended away. Waiters see a denial; nothing is
    /// journaled, because nothing was decided. A resolved gate stays.
    pub fn withdraw(&self, id: &str) -> Option<GateEntry> {
        let pending = self.slots.get(id)?.entry.resolution.is_none();
        if !pending {
            return None;
        }
        self.slots.remove(id).map(|(_, slot)| slot.entry.clone())
    }

    /// Resolve a gate. The caller has already recorded the signed decision
    /// and passes its journal event id (plus what the human said, for answer
    /// gates); waiters receive the full resolution.
    pub fn resolve(
        &self,
        id: &str,
        approve: bool,
        answer: Option<Answer>,
        decision_event_id: String,
        clarify_rounds_left: Option<u8>,
    ) -> Option<GateEntry> {
        // The slot is held for writing from the check to the swap: two
        // resolutions arriving together are one resolution.
        let mut slot = self.slots.get_mut(id)?;
        // Mark decided in a fresh entry (DashMap value is behind Arc).
        let mut entry = slot.entry.clone();
        if entry.resolution.is_some() {
            return None; // already decided; decisions are not re-decidable
        }
        let resolution = GateResolution {
            approve,
            answer,
            decision_event_id,
            clarify_rounds_left,
            withdrawn: false,
        };
        entry.resolution = Some(resolution.clone());
        let tx = slot.tx.clone();
        *slot = Arc::new(GateSlot {
            entry: entry.clone(),
            tx: tx.clone(),
        });
        drop(slot);
        // No receiver means nobody is waiting on this gate, which is not a fault.
        if tx.send(Some(resolution)).is_err() {
            tracing::trace!("gate {id} resolved with no waiter");
        }
        Some(entry)
    }

    /// Wait for a gate resolution.
    pub async fn wait(&self, mut rx: watch::Receiver<Option<GateResolution>>) -> GateResolution {
        loop {
            if let Some(d) = rx.borrow().clone() {
                return d;
            }
            if rx.changed().await.is_err() {
                // The slot went without a decision — withdrawn: a refusal
                // that is nobody's.
                return GateResolution {
                    approve: false,
                    answer: None,
                    decision_event_id: String::new(),
                    clarify_rounds_left: None,
                    withdrawn: true,
                };
            }
        }
    }
}

/// Global freeze switch, checked at safe boundaries (before scheduling a
/// work-item, before prompting, before approving a tool). Parks, never aborts.
#[derive(Clone)]
pub struct PauseGate {
    tx: Arc<watch::Sender<bool>>,
}

impl Default for PauseGate {
    fn default() -> Self {
        let (tx, _) = watch::channel(false);
        Self { tx: Arc::new(tx) }
    }
}

impl PauseGate {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn pause(&self) {
        // send_replace: a watch `send` with zero receivers is a no-op error;
        // the pause must stick regardless of who is subscribed right now.
        self.tx.send_replace(true);
    }

    pub fn resume(&self) {
        self.tx.send_replace(false);
    }

    pub fn is_paused(&self) -> bool {
        *self.tx.subscribe().borrow()
    }

    /// Wait until not paused (returns immediately when running).
    pub async fn wait_running(&self) {
        let mut rx = self.tx.subscribe();
        while *rx.borrow() {
            if rx.changed().await.is_err() {
                // LCOV_EXCL_START: the pause channel closes only when the engine is dropped under a waiter, at shutdown
                return;
                // LCOV_EXCL_STOP
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn goal() -> GoalId {
        GoalId::from_ulid(ulid::Ulid::from_parts(1, 1))
    }

    fn open(gates: &Gates) -> String {
        gates
            .open(
                Home::Goal { goal: goal() },
                None,
                Gate::Approval,
                "adopt:wf@1",
                "Adopt it?",
                AskKind::Decision,
            )
            .0
    }

    #[test]
    fn a_gate_is_decided_by_one_hand_at_a_time_and_given_back_when_the_hand_lets_go() {
        let gates = Gates::new();
        let id = open(&gates);
        let first = gates.begin_decide(&id).expect("the gate is open");
        assert_eq!(first.entry.subject, "adopt:wf@1");
        assert_eq!(
            gates.begin_decide(&id).err(),
            Some(DecideRefusal::Decided),
            "whoever comes second while it is being decided is told it is taken"
        );
        // A decision refused half way — the guard dropped, nothing resolved.
        drop(first);
        let again = gates.begin_decide(&id).expect("the gate is open again");
        assert!(gates
            .resolve(&id, true, None, "01J0DECISION".into(), None)
            .is_some());
        drop(again);
        assert_eq!(
            gates.begin_decide(&id).err(),
            Some(DecideRefusal::Decided),
            "a decided gate is never taken again"
        );
        assert_eq!(
            gates.begin_decide("01J0NOSUCHGATE").err(),
            Some(DecideRefusal::Unknown)
        );
    }

    /// A run of the workspace's gates are its own: listed by its home, and
    /// never among a goal's.
    #[test]
    fn a_gate_is_listed_by_the_home_it_is_asked_in() {
        let gates = Gates::new();
        let run = RunId::from_ulid(ulid::Ulid::from_parts(2, 2));
        let on_goal = open(&gates);
        let (on_run, _) = gates.open_for_step(
            Home::Run { run },
            run,
            StepId::new("ship").unwrap(),
            Gate::Approval,
            format!("approval:{run}/ship"),
            "Ship?",
            AskKind::Decision,
        );
        let ids = |v: Vec<GateEntry>| v.into_iter().map(|g| g.id).collect::<Vec<_>>();
        assert_eq!(ids(gates.pending_for_goal(goal())), vec![on_goal]);
        assert_eq!(
            ids(gates.pending_for_home(&Home::Run { run })),
            vec![on_run.clone()]
        );
        assert_eq!(gates.get(&on_run).unwrap().run, Some(run));
    }

    #[test]
    fn a_gate_resolves_once_whoever_asks() {
        let gates = Gates::new();
        let id = open(&gates);
        assert!(gates
            .resolve(&id, false, None, "first".into(), None)
            .is_some());
        assert!(
            gates
                .resolve(&id, true, None, "second".into(), None)
                .is_none(),
            "decisions are not re-decidable"
        );
        let kept = gates.get(&id).unwrap().resolution.unwrap();
        assert_eq!(
            (kept.approve, kept.decision_event_id.as_str()),
            (false, "first")
        );
        assert!(gates
            .resolve("01J0NOSUCHGATE", true, None, "x".into(), None)
            .is_none());
        assert!(gates.pending().is_empty());
    }

    #[test]
    fn deciders_arriving_together_are_one_decision_and_the_rest_refusals() {
        let gates = Arc::new(Gates::new());
        for _ in 0..50 {
            let id = open(&gates);
            let start = Arc::new(std::sync::Barrier::new(8));
            let hands: Vec<_> = (0..8)
                .map(|n| {
                    let (gates, id, start) = (Arc::clone(&gates), id.clone(), Arc::clone(&start));
                    std::thread::spawn(move || {
                        start.wait();
                        let Ok(_mine) = gates.begin_decide(&id) else {
                            return 0u32;
                        };
                        u32::from(
                            gates
                                .resolve(&id, n % 2 == 0, None, format!("by-{n}"), None)
                                .is_some(),
                        )
                    })
                })
                .collect();
            let decided: u32 = hands.into_iter().map(|h| h.join().unwrap()).sum();
            assert_eq!(decided, 1, "one hand decided {id}");
            assert!(gates.get(&id).unwrap().resolution.is_some());
        }
    }
}
