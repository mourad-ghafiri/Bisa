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

use crate::{executor, ops, waits, Inner};
use bisa_core::event::JournalPayload;
use bisa_core::run::{dies_with_the_process, AMBIGUOUS_WRITE, INTERRUPTED};
use bisa_core::{
    Home, RunEffect, RunEvent, StepId, StepState, WorkItemId, WorkItemState, WorkflowRun,
};
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
        Err(e) => {
            tracing::error!(target: "bisa_engine", "restart recovery cannot list the goals: {e}");
            return recovered;
        }
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
        Err(e) => {
            tracing::error!(target: "bisa_engine", "restart recovery cannot list the runs of the workspace: {e}");
        }
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
            Err(e) => {
                tracing::warn!(target: "bisa_engine", run = %run.id, step = %step, "restart recovery could not interrupt the step: {e}");
            }
        }
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
        tracing::warn!(target: "bisa_engine", %home, "restart recovery could not note the run's home: {e}");
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
/// guard's escalation, a permission. Their asker died with the process.
fn dies_with_its_asker(subject: &str) -> bool {
    subject.starts_with("workstream:")
        || subject.starts_with("guard:")
        || subject.starts_with("permission:")
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
        Err(_) => Vec::new(),
    };
    match inner.ws.live_workspace_runs(None) {
        Ok(runs) => homes.extend(runs.iter().map(WorkflowRun::home)),
        Err(e) => {
            tracing::warn!(target: "bisa_engine", "restart recovery cannot list the runs of the workspace: {e}")
        }
    }
    let mut withdrawn = 0;
    for home in homes {
        for (subject, text) in open_dead_questions(inner, &home) {
            let (signer, attestation) = ops::signer_for(&inner.ws, None);
            if let Err(e) = inner.ws.append_journal(
                &home,
                JournalPayload::Withdrawn {
                    subject: subject.clone(),
                    reason: INTERRUPTED.to_string(),
                },
                &signer,
                attestation,
            ) {
                tracing::warn!(target: "bisa_engine", %home, "restart recovery could not withdraw a question: {e}");
                continue;
            }
            let note = format!(
                "a decision you were asked for was interrupted by a restart — \"{}\" — ask again from where it came",
                text.lines().next().unwrap_or_default().chars().take(160).collect::<String>()
            );
            if let Err(e) = ops::add_note(inner, home, note, None) {
                tracing::warn!(target: "bisa_engine", %home, "restart recovery could not note a withdrawn question: {e}");
            }
            withdrawn += 1;
        }
    }
    if withdrawn > 0 {
        tracing::info!(target: "bisa_engine", withdrawn, "restart recovery withdrew the questions the last process was asking");
    }
    withdrawn
}

/// `(subject, text)` of every question on the home whose asker died with the
/// process and that no later decision or withdrawal settles.
fn open_dead_questions(inner: &Arc<Inner>, home: &Home) -> Vec<(String, String)> {
    let Ok(journal) = inner.ws.journal(home) else {
        return Vec::new();
    };
    still_open(journal.iter().map(|e| &e.payload))
}

/// The questions of a journal, oldest fact first, that nothing settled,
/// newest first. The **newest fact on a subject** says where it stands: a
/// question is open, a decision or a withdrawal is not — so a subject asked
/// about again after it was answered is open again, and one asked about
/// twice with no answer between is one question. Pure.
fn still_open<'a>(
    facts: impl DoubleEndedIterator<Item = &'a JournalPayload>,
) -> Vec<(String, String)> {
    let mut seen: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    let mut open: Vec<(String, String)> = Vec::new();
    for payload in facts.rev() {
        match payload {
            JournalPayload::Question { gate, text, .. } if dies_with_its_asker(gate) => {
                if seen.insert(gate) {
                    open.push((gate.clone(), text.clone()));
                }
            }
            JournalPayload::Decision { subject, .. }
            | JournalPayload::Withdrawn { subject, .. }
                if dies_with_its_asker(subject) =>
            {
                seen.insert(subject);
            }
            _ => {}
        }
    }
    open
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_questions_that_die_with_their_asker_are_the_three_with_nobody_durable_behind_them() {
        assert!(dies_with_its_asker("workstream:01J"));
        assert!(dies_with_its_asker("guard:sh"));
        assert!(dies_with_its_asker("permission:Bash"));
        assert!(!dies_with_its_asker("ask_human:01J"));
        assert!(!dies_with_its_asker("amend:01J@2"));
        assert!(!dies_with_its_asker("approval:01J/step"));
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
        still_open(journal.iter())
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
