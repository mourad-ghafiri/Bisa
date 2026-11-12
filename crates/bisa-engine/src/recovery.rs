//! What a restart does to the runs the last process was driving.
//!
//! A `Running` step is a promise this process holds nothing for: the
//! session, the check, the connector call or the post that was moving it
//! died with the last process, and `inner.inflight`, `active_items` and the
//! effect tasks are memory. One walk over every unfinished run — each open
//! goal's, read from the snapshots, the truth, and every run of the
//! workspace that is going, from its folder — does, per run: re-arm its
//! waits ([`crate::waits::rearm_run`]), send
//! [`RunEvent::StepInterrupted`] to each live step whose kind dies with the
//! process (the machine decides: an `agent` resumes on the work item it
//! already has, a `check` runs again, the rest run again within their
//! `retries` or fail, and none is charged an attempt for the restart), cancel
//! the work items a crash orphaned between their creation and the step that
//! would have named them, and write **one** note on the run's home — its
//! goal, or the run itself — saying what became of each step.
//!
//! The questions a dead process had open with nobody durable behind them —
//! a publish, a guard's escalation, a permission — are withdrawn with a note
//! by [`withdraw_dead_questions`], so nothing waiting on a person hides
//! behind a restarted daemon.
//!
//! `Waiting` human and approval asks are not this module's: the inbox
//! rebuilds them durably from the run. Sessions are [`crate::sessions::
//! end_stale`]'s, which runs before this walk so no child a crash left is
//! writing into a checkout the walk is about to resume.

use crate::{effects, executor, ops, waits, Inner};
use bisa_core::event::JournalPayload;
use bisa_core::run::{dies_with_the_process, AMBIGUOUS_WRITE, INTERRUPTED};
use bisa_core::{
    GoalOrigin, Home, RunEffect, RunEvent, StepId, StepKind, StepState, WorkItemId, WorkItemState,
    WorkflowRun,
};
use std::collections::HashSet;
use std::sync::Arc;

/// The reason an item created a moment before the crash, that no step names, is cancelled with.
pub const ORPHANED: &str = "orphaned by a restart";

/// What the walk did, for the boot line.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Recovered {
    pub runs: usize,
    pub steps: usize,
    pub orphans: usize,
    pub questions: usize,
    /// Queued runs started because their goal had nothing live at boot: the
    /// last process ended a run and stopped before its queue advanced.
    pub queued_started: usize,
}

/// What became of one interrupted step, for the home's note.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Fate {
    Resumed,
    RunsAgain,
    Failed,
}

impl Fate {
    fn of(effects: &[RunEffect], run: &WorkflowRun, step: &StepId) -> Fate {
        if effects
            .iter()
            .any(|e| matches!(e, RunEffect::ResumeAgent { step: s, .. } if s == step))
        {
            return Fate::Resumed;
        }
        match run.steps.get(step).map(|r| &r.state) {
            Some(StepState::Running) => Fate::RunsAgain,
            _ => Fate::Failed,
        }
    }

    fn words(&self, step: &StepId) -> String {
        match self {
            Fate::Resumed => format!("`{step}` resumed on its work item"),
            Fate::RunsAgain => format!("`{step}` will run again"),
            Fate::Failed => format!("`{step}` failed — no retries left"),
        }
    }
}

/// The walk. Errors are logged and never stop it: one goal or run that
/// cannot be read costs that one, not the rest.
pub fn sweep(inner: &Arc<Inner>) -> Recovered {
    let mut recovered = Recovered::default();
    let goals = match inner.ws.list_goals(None) {
        Ok(goals) => goals,
        // LCOV_EXCL_START: a goal list that fails fails the boot's earlier walks first; every broken file it meets is tolerated by name
        Err(e) => {
            tracing::error!(target: "bisa_engine", "restart recovery cannot list the goals: {e}");
            return recovered;
        } // LCOV_EXCL_STOP
    };
    let goals: Vec<_> = goals.into_iter().filter(|g| !g.is_closed()).collect();
    for goal in &goals {
        let Some(run_id) = goal.run else {
            continue;
        };
        match inner.ws.get_run(run_id) {
            Ok(run) if !run.is_finished() => recover_run(inner, &run, &mut recovered),
            Ok(_) => {}
            Err(e) => {
                tracing::error!(target: "bisa_engine", goal = %goal.id, run = %run_id, "restart recovery cannot read the run: {e}");
            }
        }
    }
    match inner.ws.live_workspace_runs(None) {
        Ok(runs) => {
            for run in &runs {
                recover_run(inner, run, &mut recovered);
            }
        }
        // LCOV_EXCL_START: the workspace's run list tolerates every broken file by name; only an index failure lands here
        Err(e) => {
            tracing::error!(target: "bisa_engine", "restart recovery cannot list the runs of the workspace: {e}");
        } // LCOV_EXCL_STOP
    }
    waits::rearm_children(inner);
    // A goal whose run ended just before the last process stopped may hold
    // a queue nothing advanced: its next run starts now.
    for goal in &goals {
        let idle = match inner.ws.live_run(goal.id) {
            Ok(live) => live.is_none(),
            Err(e) => {
                tracing::warn!(target: "bisa_engine", goal = %goal.id, "restart recovery cannot read the goal's live run: {e}");
                continue;
            }
        };
        if idle && ops::advance_queue(inner, goal.id).is_some() {
            recovered.queued_started += 1;
        }
    }
    if recovered.steps > 0 || recovered.orphans > 0 || recovered.queued_started > 0 {
        tracing::info!(target: "bisa_engine", runs = recovered.runs, steps = recovered.steps, orphans = recovered.orphans, queued_started = recovered.queued_started, "restart recovery walked the runs the last process was driving");
    }
    recovered
}

/// One unfinished run, as a restart finds it: its waits re-armed, its
/// orphans cancelled, every step that died with the process interrupted
/// through the funnel, and one note on its home saying what became of each.
fn recover_run(inner: &Arc<Inner>, run: &WorkflowRun, recovered: &mut Recovered) {
    recovered.runs += 1;
    let home = run.home();
    waits::rearm_run(inner, run);
    // The orphans first, read off the run as the crash left it: an item the
    // interruptions below make is named by its step the moment it exists,
    // and must never be mistaken for one the crash orphaned.
    let orphans = orphaned_items(inner, run);
    for item in &orphans {
        executor::cancel_item(inner, &home, *item, ORPHANED);
    }
    recovered.orphans += orphans.len();
    let mut fates: Vec<String> = Vec::new();
    for step in interrupted_steps(run) {
        // A connector write with no idempotency key may already have reached
        // the platform: it is stopped, never sent again on the platform's own
        // — the person looks first.
        let outcome = if crate::connectors::unsafe_to_resend(inner, run, &step) {
            ops::record_run_event_with_effects(
                inner,
                run.id,
                RunEvent::StepStopped {
                    step: step.clone(),
                    error: format!(
                        "{INTERRUPTED} while writing to the platform; {AMBIGUOUS_WRITE}"
                    ),
                },
            )
            .map(|(after, effects)| (after, effects, true))
        } else {
            record_interrupted(inner, run.id, &step).map(|(after, effects)| (after, effects, false))
        };
        match outcome {
            Ok((after, effects, stopped)) => {
                recovered.steps += 1;
                fates.push(if stopped {
                    format!("`{step}` stopped — a write that may have reached the platform is not sent again")
                } else {
                    Fate::of(&effects, &after, &step).words(&step)
                });
            }
            // The run moved under the walk — the first interruption finished
            // it, say — and the machine refused: not an error.
            Err(e) if e.is_refusal() => {
                tracing::debug!(target: "bisa_engine", run = %run.id, step = %step, "restart recovery skipped a step: {e}");
            }
            // LCOV_EXCL_START: a store that cannot write the run's snapshot; a refusal is the arm above
            Err(e) => {
                tracing::warn!(target: "bisa_engine", run = %run.id, step = %step, "restart recovery could not interrupt the step: {e}");
            } // LCOV_EXCL_STOP
        }
    }
    // A `spawn` left waiting with no child of its own — the crash fell between
    // the snapshot that entered it and the effect that makes the child —
    // spawns it now: the effect runs again, as a check's would.
    for step in unmade_spawns(inner, run) {
        effects::run_effects(
            inner,
            run,
            vec![RunEffect::SpawnGoal { step: step.clone() }],
        );
        recovered.steps += 1;
        fates.push(format!("`{step}` spawned its child anew"));
    }
    if fates.is_empty() && orphans.is_empty() {
        return;
    }
    let mut text = match fates.len() {
        0 => String::new(),
        n => format!(
            "a restart interrupted {n} running step{}: {}",
            if n == 1 { "" } else { "s" },
            fates.join(", ")
        ),
    };
    if !orphans.is_empty() {
        if !text.is_empty() {
            text.push_str("; ");
        }
        text.push_str(&format!(
            "{} work item{} created just before it, named by no step, cancelled",
            orphans.len(),
            if orphans.len() == 1 { "" } else { "s" }
        ));
    }
    if let Err(e) = ops::add_note(inner, home, text, None) {
        // LCOV_EXCL_START: the journal that just took the run's facts refuses a note only on a disk that fails between the two
        tracing::warn!(target: "bisa_engine", %home, "restart recovery could not note the run's home: {e}");
        // LCOV_EXCL_STOP
    }
}

/// Send the interruption through the funnel and read what the machine did:
/// the run after, and the effects the walk can name a fate from.
fn record_interrupted(
    inner: &Arc<Inner>,
    run: bisa_core::RunId,
    step: &StepId,
) -> Result<(WorkflowRun, Vec<RunEffect>), crate::EngineError> {
    ops::record_run_event_with_effects(inner, run, RunEvent::StepInterrupted { step: step.clone() })
}

/// The run's `spawn` steps waiting on a child that was never made: no goal
/// names the step in its origin, and — for a child from before the step was
/// named — none names the run's goal without a step either, each such child
/// standing for the first waiting spawn it could be.
fn unmade_spawns(inner: &Arc<Inner>, run: &WorkflowRun) -> Vec<StepId> {
    let waiting: Vec<StepId> = run
        .workflow
        .steps
        .iter()
        .filter(|def| matches!(def.kind, StepKind::Spawn { wait: true, .. }))
        .filter(|def| {
            run.steps
                .get(&def.id)
                .is_some_and(|record| record.state == StepState::Waiting)
        })
        .map(|def| def.id.clone())
        .collect();
    if waiting.is_empty() {
        return Vec::new();
    }
    let Ok(goals) = inner.ws.list_goals(None) else {
        return Vec::new(); // LCOV_EXCL_LINE the goals list is read from the index, which fails only unreadable (disk-only)
    };
    let mut named: HashSet<StepId> = HashSet::new();
    let mut unnamed = 0usize;
    for goal in goals {
        match &goal.origin {
            GoalOrigin::Spawned { parent, step } if Some(*parent) == run.scope.goal() => match step
            {
                Some(step) => {
                    named.insert(step.clone());
                }
                None => unnamed += 1,
            },
            GoalOrigin::Run { run: of, step } if *of == run.id => {
                named.insert(step.clone());
            }
            _ => {}
        }
    }
    waiting
        .into_iter()
        .filter(|step| {
            if named.contains(step) {
                return false;
            }
            if unnamed > 0 {
                unnamed -= 1;
                return false;
            }
            true
        })
        .collect()
}

/// The steps a restart interrupted, in definition order. Pure.
fn interrupted_steps(run: &WorkflowRun) -> Vec<StepId> {
    run.workflow
        .steps
        .iter()
        .filter(|def| dies_with_the_process(&def.kind))
        .filter_map(|def| {
            let record = run.steps.get(&def.id)?;
            (record.state == StepState::Running).then(|| def.id.clone())
        })
        .collect()
}

/// The run's work items that are neither settled nor named by their step's
/// record — created by a `StartAgent` a crash cut off before `StepStarted`
/// bound them. Nothing will ever launch or settle them; they are cancelled
/// so the board does not show work nobody is doing.
fn orphaned_items(inner: &Arc<Inner>, run: &WorkflowRun) -> Vec<WorkItemId> {
    let Ok(items) = inner.ws.list_work_items(&run.home()) else {
        return Vec::new();
    };
    items
        .into_iter()
        .filter(|item| item.run == Some(run.id))
        .filter(|item| {
            matches!(
                item.state,
                WorkItemState::Open
                    | WorkItemState::Claimed { .. }
                    | WorkItemState::InProgress { .. }
            )
        })
        .filter(|item| {
            let named = item
                .step
                .as_ref()
                .and_then(|s| run.steps.get(s))
                .is_some_and(|record| record.work_item == Some(item.id));
            !named
        })
        .map(|item| item.id)
        .collect()
}

/// The subjects of questions no durable ask stands behind: a publish, a
/// guard's escalation, a permission — and a work item's own question
/// (`ask_human:<work item>`): its session died with the process, the item
/// runs again and asks again, and an answer to the old one would reach
/// nobody. The home's own question (`ask_human:<home>`, the designer's)
/// stays: the design is told the answer when it comes.
fn dies_with_its_asker(subject: &str, home: &Home) -> bool {
    subject.starts_with("workstream:")
        || subject.starts_with("guard:")
        || subject.starts_with("permission:")
        || subject
            .strip_prefix("ask_human:")
            .is_some_and(|asker| asker != home.id())
}

/// Withdraw every such question still open on an unclosed goal or a run of
/// the workspace that is going, with a note naming it, so the person asks
/// again from where the question came.
pub fn withdraw_dead_questions(inner: &Arc<Inner>) -> usize {
    let mut homes: Vec<Home> = match inner.ws.list_goals(None) {
        Ok(goals) => goals
            .into_iter()
            .filter(|g| !g.is_closed())
            .map(|g| Home::Goal { goal: g.id })
            .collect(),
        Err(_) => Vec::new(), // LCOV_EXCL_LINE: a goal list that fails fails the boot's earlier walks first; every broken file it meets is tolerated by name
    };
    match inner.ws.live_workspace_runs(None) {
        Ok(runs) => homes.extend(runs.iter().map(WorkflowRun::home)),
        // LCOV_EXCL_START: the workspace's run list tolerates every broken file by name; only an index failure lands here
        Err(e) => {
            tracing::warn!(target: "bisa_engine", "restart recovery cannot list the runs of the workspace: {e}")
        } // LCOV_EXCL_STOP
    }
    let mut withdrawn = 0;
    for home in homes {
        for (subject, text) in open_dead_questions(inner, &home) {
            let note = format!(
                "a decision you were asked for was interrupted by a restart — \"{}\" — ask again from where it came",
                first_line(&text)
            );
            if withdraw_question(inner, home, &subject, INTERRUPTED, Some(note)) {
                withdrawn += 1;
            }
        }
    }
    if withdrawn > 0 {
        tracing::info!(target: "bisa_engine", withdrawn, "restart recovery withdrew the questions the last process was asking");
    }
    withdrawn
}

/// The first line of a question, as a note quotes it.
pub(crate) fn first_line(text: &str) -> String {
    text.lines()
        .next()
        .unwrap_or_default()
        .chars()
        .take(160)
        .collect()
}

/// Withdraw one journaled question on `home` without deciding it: a
/// `Withdrawn` fact under its subject with `reason`, and — when one is given
/// — `note` on the home saying so. The one way a question that was journaled
/// is taken back: the Inbox reads the withdrawal as a decision would be, the
/// durable decide finds nothing to decide, and a later boot does not withdraw
/// it again. The live gate, when one stands, is the caller's to withdraw.
/// Answers whether the fact was written; a journal that cannot be written is
/// logged, never a panic.
pub(crate) fn withdraw_question(
    inner: &Arc<Inner>,
    home: Home,
    subject: &str,
    reason: &str,
    note: Option<String>,
) -> bool {
    let (signer, attestation) = ops::signer_or_owner(&inner.ws, None);
    if let Err(e) = inner.ws.append_journal(
        &home,
        JournalPayload::Withdrawn {
            subject: subject.to_string(),
            reason: reason.to_string(),
        },
        &signer,
        attestation,
    ) {
        tracing::warn!(target: "bisa_engine", %home, "could not withdraw a question: {e}");
        return false;
    }
    if let Some(note) = note {
        if let Err(e) = ops::add_note(inner, home, note, None) {
            // LCOV_EXCL_START: the journal that just took the withdrawal refuses a note only on a disk that fails between the two
            tracing::warn!(target: "bisa_engine", %home, "could not note a withdrawn question: {e}");
            // LCOV_EXCL_STOP
        }
    }
    true
}

/// `(subject, text)` of every question on the home whose asker died with the
/// process and that no later decision or withdrawal settles.
fn open_dead_questions(inner: &Arc<Inner>, home: &Home) -> Vec<(String, String)> {
    let Ok(journal) = inner.ws.journal(home) else {
        return Vec::new();
    };
    still_open(journal.iter().map(|e| &e.payload), |subject| {
        dies_with_its_asker(subject, home)
    })
}

/// The questions of a journal, oldest fact first, that nothing settled,
/// newest first — among the subjects `applies` picks out. The **newest fact
/// on a subject** says where it stands: a question is open, a decision or a
/// withdrawal is not — so a subject asked about again after it was answered
/// is open again, and one asked about twice with no answer between is one
/// question. Pure.
pub(crate) fn still_open<'a>(
    facts: impl DoubleEndedIterator<Item = &'a JournalPayload>,
    applies: impl Fn(&str) -> bool,
) -> Vec<(String, String)> {
    let mut seen: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    let mut open: Vec<(String, String)> = Vec::new();
    for payload in facts.rev() {
        match payload {
            JournalPayload::Question { gate, text, .. } if applies(gate) => {
                if seen.insert(gate) {
                    open.push((gate.clone(), text.clone()));
                }
            }
            JournalPayload::Decision { subject, .. }
            | JournalPayload::Withdrawn { subject, .. }
                if applies(subject) =>
            {
                seen.insert(subject);
            }
            _ => {}
        }
    }
    open
}

/// One question a journal still holds open: what was asked, how it may be
/// answered, by which work item, and when.
#[derive(Clone, Debug, PartialEq)]
pub struct OpenQuestion {
    pub subject: String,
    pub text: String,
    pub expects: bisa_core::AskKind,
    pub work_item: Option<bisa_core::WorkItemId>,
    pub at: u64,
}

/// [`still_open`] with everything a card needs: the questions nothing
/// settled, newest first, among the subjects `applies` picks out — the one
/// reader the Inbox rebuilds its asks from and the durable decide finds its
/// question with, so what is shown is what can be decided.
///
/// `applies` names a **family of subjects** (`adopt:`, `amend:`,
/// `ask_human:` …), never everything: a step's own question is journaled
/// under its gate's id while its decision is filed under
/// `approval:<run>/<step>` or `step:<run>/<step>`, so no fact ever settles
/// that id and a reader of every subject would call each one open for good.
/// A step's question is read from its run, where its state says it waits.
pub fn open_questions(
    journal: &[bisa_core::event::JournalEvent],
    applies: impl Fn(&str) -> bool,
) -> Vec<OpenQuestion> {
    let mut seen: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    let mut open = Vec::new();
    for e in journal.iter().rev() {
        match &e.payload {
            JournalPayload::Question {
                work_item,
                gate,
                text,
                expects,
            } if applies(gate) => {
                if seen.insert(gate) {
                    open.push(OpenQuestion {
                        subject: gate.clone(),
                        text: text.clone(),
                        expects: expects.clone(),
                        work_item: *work_item,
                        at: e.at,
                    });
                }
            }
            JournalPayload::Decision { subject, .. }
            | JournalPayload::Withdrawn { subject, .. }
                if applies(subject) =>
            {
                seen.insert(subject);
            }
            _ => {}
        }
    }
    open
}

/// Where one subject stands in a journal: its newest fact says.
#[derive(Clone, Debug, PartialEq)]
pub enum QuestionStanding {
    /// Asked, and nothing settled it since.
    Open(OpenQuestion),
    /// A decision is its newest fact.
    Decided,
    /// Taken back — by the asker, a person's own act, or a restart.
    Withdrawn,
    /// The journal holds no fact on it.
    NeverAsked,
}

/// The standing of `subject` alone — what a decision that names its
/// question asks before it records anything.
pub fn standing_of(journal: &[bisa_core::event::JournalEvent], subject: &str) -> QuestionStanding {
    for e in journal.iter().rev() {
        match &e.payload {
            JournalPayload::Question {
                work_item,
                gate,
                text,
                expects,
            } if gate == subject => {
                return QuestionStanding::Open(OpenQuestion {
                    subject: gate.clone(),
                    text: text.clone(),
                    expects: expects.clone(),
                    work_item: *work_item,
                    at: e.at,
                });
            }
            JournalPayload::Decision { subject: s, .. } if s == subject => {
                return QuestionStanding::Decided;
            }
            JournalPayload::Withdrawn { subject: s, .. } if s == subject => {
                return QuestionStanding::Withdrawn;
            }
            _ => {}
        }
    }
    QuestionStanding::NeverAsked
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(at: u64, payload: JournalPayload) -> bisa_core::event::JournalEvent {
        bisa_core::event::JournalEvent {
            home: Home::Run {
                run: bisa_core::RunId::from_ulid(ulid::Ulid::from_parts(4, 1)),
            },
            author: bisa_core::PrincipalId::new("ab".repeat(32)).unwrap(),
            at,
            payload,
        }
    }

    /// Two work items' questions stand apart: answering the newer leaves the
    /// older open — the reader that stopped at the first settled fact of the
    /// family hid it for good — and each subject's own newest fact says
    /// where it stands.
    #[test]
    fn two_work_items_questions_stand_apart() {
        let journal = vec![
            event(1, asked("ask_human:W1", "one?")),
            event(2, asked("ask_human:W2", "two?")),
            event(3, decided("ask_human:W2")),
        ];
        let family = |s: &str| s.starts_with("ask_human:");
        let open = open_questions(&journal, family);
        assert_eq!(open.len(), 1);
        assert_eq!(
            (open[0].subject.as_str(), open[0].text.as_str(), open[0].at),
            ("ask_human:W1", "one?", 1)
        );
        assert!(matches!(
            standing_of(&journal, "ask_human:W1"),
            QuestionStanding::Open(q) if q.text == "one?"
        ));
        assert_eq!(
            standing_of(&journal, "ask_human:W2"),
            QuestionStanding::Decided
        );
        assert_eq!(
            standing_of(&journal, "ask_human:W3"),
            QuestionStanding::NeverAsked
        );
        // Asked again after its answer: open again; taken back: withdrawn.
        let mut again = journal.clone();
        again.push(event(4, asked("ask_human:W2", "two, again?")));
        again.push(event(
            5,
            JournalPayload::Withdrawn {
                subject: "ask_human:W1".into(),
                reason: "stopped".into(),
            },
        ));
        let open = open_questions(&again, family);
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].text, "two, again?");
        assert_eq!(
            standing_of(&again, "ask_human:W1"),
            QuestionStanding::Withdrawn
        );
        // The two readers agree on what is open.
        let subjects: Vec<String> = still_open(again.iter().map(|e| &e.payload), family)
            .into_iter()
            .map(|(subject, _)| subject)
            .collect();
        assert_eq!(subjects, vec!["ask_human:W2".to_string()]);
    }

    /// A step's question is journaled under its gate's id and decided under
    /// `step:<run>/<step>`: nothing ever settles the id, so a family is
    /// always named — a reader of every subject calls the step's question
    /// open after its answer.
    #[test]
    fn a_steps_question_under_its_gate_id_is_nobodys_open_question() {
        let journal = vec![
            event(1, asked("01GATEULID", "Which database?")),
            event(2, decided("step:01RUN/ask")),
        ];
        for family in ["adopt:", "amend:", "ask_human:"] {
            assert!(open_questions(&journal, |s| s.starts_with(family)).is_empty());
        }
        assert_eq!(
            open_questions(&journal, |_| true).len(),
            1,
            "why no caller asks for every subject"
        );
    }

    #[test]
    fn the_questions_that_die_with_their_asker_are_the_three_with_nobody_durable_behind_them() {
        let goal = bisa_core::GoalId::from_ulid(ulid::Ulid::from_parts(2, 2));
        let home = Home::Goal { goal };
        assert!(dies_with_its_asker("workstream:01J", &home));
        assert!(dies_with_its_asker("guard:sh", &home));
        assert!(dies_with_its_asker("permission:Bash", &home));
        // A work item's question dies with its session; the home's own —
        // the designer's — is answered when the answer comes.
        assert!(dies_with_its_asker("ask_human:01J", &home));
        assert!(!dies_with_its_asker(&format!("ask_human:{goal}"), &home));
        assert!(!dies_with_its_asker("amend:01J@2", &home));
        assert!(!dies_with_its_asker("approval:01J/step", &home));
    }

    fn asked(subject: &str, text: &str) -> JournalPayload {
        JournalPayload::Question {
            work_item: None,
            gate: subject.into(),
            text: text.into(),
            expects: bisa_core::AskKind::Decision,
        }
    }

    fn decided(subject: &str) -> JournalPayload {
        JournalPayload::Decision {
            gate: bisa_core::Gate::Approval,
            approve: true,
            subject: subject.into(),
            rationale: None,
            answer: None,
        }
    }

    fn withdrawn(subject: &str) -> JournalPayload {
        JournalPayload::Withdrawn {
            subject: subject.into(),
            reason: INTERRUPTED.into(),
        }
    }

    fn open_in(journal: &[JournalPayload]) -> Vec<(String, String)> {
        let home = Home::Goal {
            goal: bisa_core::GoalId::from_ulid(ulid::Ulid::from_parts(2, 2)),
        };
        still_open(journal.iter(), |subject| {
            dies_with_its_asker(subject, &home)
        })
    }

    #[test]
    fn a_question_is_open_until_a_decision_or_a_withdrawal_after_it_on_its_subject() {
        let one = |subject: &str, text: &str| vec![(subject.to_string(), text.to_string())];

        assert_eq!(
            open_in(&[asked("permission:Bash", "run it?")]),
            one("permission:Bash", "run it?")
        );
        assert!(open_in(&[
            asked("permission:Bash", "run it?"),
            decided("permission:Bash")
        ])
        .is_empty());
        assert!(open_in(&[asked("guard:sh", "run it?"), withdrawn("guard:sh")]).is_empty());
        // A decision on another subject settles nothing here.
        assert_eq!(
            open_in(&[
                asked("permission:Bash", "run it?"),
                decided("permission:Edit")
            ]),
            one("permission:Bash", "run it?")
        );
        // A question with somebody durable behind it is the inbox's to
        // rebuild, never this walk's to withdraw.
        assert!(open_in(&[asked("approval:01J/step", "ship it?")]).is_empty());
    }

    /// The same tool is asked about many times in one home: the answer to the
    /// first question is no answer to the second.
    #[test]
    fn a_question_asked_again_after_its_subject_was_decided_is_open() {
        let journal = [
            asked("permission:Bash", "the first"),
            decided("permission:Bash"),
            asked("permission:Bash", "the second"),
        ];
        assert_eq!(
            open_in(&journal),
            vec![("permission:Bash".to_string(), "the second".to_string())]
        );

        let journal = [
            asked("permission:Bash", "the first"),
            withdrawn("permission:Bash"),
            asked("permission:Bash", "the second"),
            asked("workstream:01J", "publish?"),
        ];
        assert_eq!(
            open_in(&journal),
            vec![
                ("workstream:01J".to_string(), "publish?".to_string()),
                ("permission:Bash".to_string(), "the second".to_string()),
            ],
            "newest first, one a subject"
        );
    }

    /// Asked twice with no answer between: one question stands, the newest.
    #[test]
    fn a_subject_asked_twice_with_no_answer_is_one_question_the_newest() {
        let journal = [
            asked("guard:sh", "the first"),
            asked("guard:sh", "the second"),
        ];
        assert_eq!(
            open_in(&journal),
            vec![("guard:sh".to_string(), "the second".to_string())]
        );
    }
}
