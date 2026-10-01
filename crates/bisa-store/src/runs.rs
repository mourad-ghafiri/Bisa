//! Workflow runs (kind 33413): the only execution state there is, and the one
//! writer of it.
//!
//! A run is an addressable snapshot filed in its **home** — its goal's
//! `state/`, or, for a run of the workspace, its own folder under
//! `workflows/runs/<RunId>/` — and every change is also a fact on that home's
//! journal (kind 3411), so the history reads without the snapshot.
//! **Nothing here decides how a run moves**: [`Workspace::record_run_event`]
//! hands the event to [`WorkflowRun::apply`] and writes whatever comes back —
//! after verifying that a `Decided` event names a decision the journal really
//! holds, signed by someone the governance policy admits. The effects that
//! come back are the engine's to run.
//!
//! **A goal has at most one live run.** A goal's run made while one is live —
//! or while others already wait — is written *queued*, with no `Start`
//! applied, and starts through [`Workspace::start_queued_run`] when the engine
//! advances the goal's queue, in `(queued_at, id)` order. **A run of the
//! workspace never queues**: it is started the moment it is made, beside any
//! other. Every writer of a run takes `run_writes`, so two starts, a start and
//! a cancel, or an event and a person can never make two of a goal's runs
//! live at once.
//!
//! A run begins at one way in ([`RunEntry`]): a start step an event began it
//! at, or — by hand — the manual entry. One made from a queued signal names
//! it (`dispatched`), and the index holds that name unique: a dispatch
//! replayed after a crash makes no second run.

use crate::error::StoreError;
use crate::index::{RunRow, RunStepRow};
use crate::journal::{EventLog, JournalAddr};
use crate::paths::Paths;
use crate::syntax::StoreSyntaxChecks;
use crate::workspace::{mint_ulid, now_secs, StoreEvent, Workspace};
use bisa_core::event::{JournalEvent, JournalPayload, RunFact, StepFact};
use bisa_core::kind::KIND_WORKFLOW_RUN;
use bisa_core::{
    Budget, CancelCause, Closure, ClosureReason, Gate, Goal, GoalId, RunEffect, RunEntry, RunEvent,
    RunId, RunScope, StepId, StepKind, StepRecord, StepState, WaitFor, Workflow, WorkflowId,
    WorkflowRun,
};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::MutexGuard;

/// A run a close cancelled, with the effects the engine settles it by.
pub type CancelledRun = (WorkflowRun, Vec<RunEffect>);

/// The subject a decision on an `approval` step carries: `<run>:<step>`.
pub fn approval_subject(run: RunId, step: &StepId) -> String {
    format!("approval:{run}/{step}")
}

/// What a start needs once the definition passed every check: the frozen
/// copy and the inputs bound.
struct Startable {
    workflow: Workflow,
    inputs: BTreeMap<String, Value>,
}

/// How a new run came to be, beyond its definition and inputs: where it
/// begins and on what, and the queued signal it was dispatched from.
struct Making {
    entry: RunEntry,
    dispatched: Option<String>,
}

impl Workspace {
    /// The one writer at a time. A poisoned lock is taken over: the run on
    /// disk is whatever the last write left, and the next write re-reads it.
    fn run_writer(&self) -> MutexGuard<'_, ()> {
        self.run_writes.lock().unwrap_or_else(|poisoned| {
            tracing::warn!("run write lock was poisoned; continuing with the run as it stands");
            poisoned.into_inner()
        })
    }

    /// Make a run of `workflow` for `scope`.
    ///
    /// **A goal's run** is started at once when the goal has no live run,
    /// queued behind it otherwise. A goal with queued runs and nothing live —
    /// between a run's end and its queue's advance, or after a restart's
    /// cancel — starts the new run ahead of the queue: a person's start, and
    /// a restart, never wait behind the queue. The goal's `runs` gains it; on
    /// an idle goal `Start` is applied and the goal's `workflow` and `run` are
    /// set, and the effects are the start's. A queued run has none.
    ///
    /// **A run of the workspace** is started at once, whatever else of the
    /// workflow is live, and filed in its own folder with the ceiling its
    /// scope carries.
    ///
    /// Refusals, in order: a signal that already began its run
    /// (`AlreadyDispatched`); for a goal, an unknown or closed goal and a busy
    /// goal asked for a workflow other than its own (a queued run is always
    /// of the goal's workflow); an unknown or archived workflow; a design
    /// made for another goal — for the workspace, a design made for any goal;
    /// a workflow with problems; an entry that is no start of it, or none by
    /// hand for a workflow only events begin; a required input missing, an
    /// input of the wrong kind or one the definition does not declare
    /// ([`bisa_core::Workflow::bind_inputs`]); what only the bound values and
    /// the scope can be wrong about ([`bisa_core::Workflow::validate_bound`] —
    /// a run of the workspace refuses a definition that reads its goal).
    /// Defaults are filled in. The frozen copy is written at revision 1; the
    /// event that began the run, when one did, is journaled on its home.
    pub fn create_run(
        &self,
        scope: RunScope,
        workflow: WorkflowId,
        inputs: BTreeMap<String, Value>,
        entry: RunEntry,
        dispatched: Option<String>,
    ) -> Result<(WorkflowRun, Vec<RunEffect>), StoreError> {
        let _one_writer = self.run_writer();
        if let Some(signal) = &dispatched {
            let began = self.idx().run_dispatched_from(signal)?;
            if let Some(run) = began {
                return Err(StoreError::AlreadyDispatched {
                    signal: signal.clone(),
                    run,
                });
            }
        }
        let making = Making { entry, dispatched };
        match scope {
            RunScope::Goal { goal } => self.create_goal_run(goal, workflow, inputs, making),
            RunScope::Workspace { budget } => {
                self.create_workspace_run(budget, workflow, inputs, making)
            }
        }
    }

    /// The checks every start makes of the definition, for the scope it is
    /// started in, before anything is written.
    fn startable(
        &self,
        scope: &RunScope,
        workflow: WorkflowId,
        inputs: BTreeMap<String, Value>,
        entry: &RunEntry,
    ) -> Result<Startable, StoreError> {
        let wf = self.get_workflow(workflow)?;
        if wf.is_archived() {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-workflow-archived-unarchive-before-running",
                a0 = (wf.name).to_string()
            )));
        }
        if let Some(designed_for) = wf.origin.goal() {
            if scope.goal() != Some(designed_for) {
                return Err(StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-workflow-was-designed-goal-promote-library-first",
                    a0 = (wf.id).to_string(),
                    designed_for = designed_for.to_string()
                )));
            }
        }
        let problems = self.validate_workflow(&wf)?;
        if !problems.is_empty() {
            return Err(StoreError::WorkflowInvalid(problems));
        }
        // Where the run begins: a start of this definition, or — by hand —
        // the manual entry, which a workflow only events begin does not have.
        match &entry.step {
            Some(step) if !wf.is_entry(step) => {
                return Err(StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-run-entry-not-a-start",
                    step = step.to_string(),
                    workflow = wf.name.clone()
                )));
            }
            None if wf.manual_entry().is_none() => {
                return Err(StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-run-no-manual-entry",
                    workflow = wf.name.clone()
                )));
            }
            _ => {}
        }
        let inputs = wf.bind_inputs(inputs)?;
        // What the bound values and the scope alone can be wrong about — a
        // cron, a delay, a conversation read from an input; a goal read in a
        // run that has none — is refused here, before anything is written,
        // rather than when the step is armed.
        let accounts = self.list_all_connector_accounts()?;
        let problems = wf.validate_bound(scope, &inputs, &accounts, &StoreSyntaxChecks);
        if !problems.is_empty() {
            return Err(StoreError::WorkflowInvalid(problems));
        }
        Ok(Startable {
            workflow: wf,
            inputs,
        })
    }

    /// A run not started yet, from what the checks let through and how it
    /// came to be.
    fn new_run(
        scope: RunScope,
        startable: Startable,
        making: Making,
        at: u64,
    ) -> (WorkflowRun, Workflow) {
        let Startable {
            workflow: wf,
            inputs,
        } = startable;
        let mut run = WorkflowRun::new(
            RunId::from_ulid(mint_ulid()),
            scope,
            wf.clone(),
            inputs,
            making.entry,
            at,
        );
        run.dispatched = making.dispatched;
        (run, wf)
    }

    /// The fact that a run began, as its home's journal records it.
    fn started_fact(run: &WorkflowRun) -> RunFact {
        RunFact::Started {
            workflow: run.workflow.id,
            revision: run.workflow.revision,
            start: run.start.clone(),
            signal: run.event.as_ref().map(|e| e.id.clone()),
        }
    }

    /// The occurrence that began a run, journaled on its home (kind 3410).
    fn journal_event_of(&self, run: &WorkflowRun) -> Result<(), StoreError> {
        match &run.event {
            Some(event) => self.journal_signal(&run.home(), event),
            None => Ok(()),
        }
    }

    fn create_goal_run(
        &self,
        goal_id: GoalId,
        workflow: WorkflowId,
        inputs: BTreeMap<String, Value>,
        making: Making,
    ) -> Result<(WorkflowRun, Vec<RunEffect>), StoreError> {
        let mut goal = self.get_goal(goal_id)?;
        if goal.is_closed() {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-goal-closed-nothing-runs",
                goal_id = goal_id.to_string()
            )));
        }
        let queues = self.live_run_of(&goal)?.is_some();
        if self.goal_is_busy(&goal)? && goal.workflow != Some(workflow) {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-goal-running-workflow-run-made-while-busy",
                goal_id = goal_id.to_string(),
                a0 = (goal.workflow.map(|w| w.to_string()).unwrap_or_default()).to_string()
            )));
        }
        let scope = RunScope::Goal { goal: goal_id };
        let startable = self.startable(&scope, workflow, inputs, &making.entry)?;
        let at = now_secs();
        let (mut run, wf) = Self::new_run(scope, startable, making, at);
        let addr = self.goal_addr(&goal);
        if queues {
            run.revision = 1;
            self.write_run_snapshot(&run, 0, at)?;
            self.journal_run_fact(
                &addr,
                RunFact::Queued {
                    workflow: wf.id,
                    revision: wf.revision,
                },
                run.id,
            )?;
            self.index_run(&run)?;
            goal.runs.push(run.id);
            self.trim_goal_runs(&mut goal)?;
            goal.revision += 1;
            self.write_goal_snapshot(&goal, at)?;
            self.index_goal(&goal, None)?;
            return Ok((run, Vec::new()));
        }
        let before = run.steps.clone();
        let effects = run.apply(RunEvent::Start, at)?;
        run.revision = 1;
        self.write_run_snapshot(&run, 0, at)?;
        self.journal_event_of(&run)?;
        self.journal_run_fact(&addr, Self::started_fact(&run), run.id)?;
        self.journal_step_facts(&addr, &run, &before)?;
        if run.is_finished() {
            self.journal_finish_fact(&addr, &run)?;
        }
        self.index_run(&run)?;

        goal.workflow = Some(workflow);
        goal.run = Some(run.id);
        goal.runs.push(run.id);
        self.trim_goal_runs(&mut goal)?;
        goal.revision += 1;
        self.write_goal_snapshot(&goal, at)?;
        self.index_goal(&goal, Some(&run))?;
        Ok((run, effects))
    }

    /// Keep the goal's snapshot to its queued runs and the newest ones; the
    /// index holds every run it has had.
    fn trim_goal_runs(&self, goal: &mut Goal) -> Result<(), StoreError> {
        let queued: std::collections::BTreeSet<RunId> = self
            .idx()
            .queued_run_ids(&goal.id.to_string())?
            .iter()
            .filter_map(|id| id.parse().ok())
            .collect();
        goal.trim_runs(&queued);
        Ok(())
    }

    /// A run of the workspace: started the moment it is made, in its own
    /// folder. Indexed before its first fact is journaled, so the fact's feed
    /// row finds the workflow it files under.
    fn create_workspace_run(
        &self,
        budget: Budget,
        workflow: WorkflowId,
        inputs: BTreeMap<String, Value>,
        making: Making,
    ) -> Result<(WorkflowRun, Vec<RunEffect>), StoreError> {
        let scope = RunScope::Workspace { budget };
        let startable = self.startable(&scope, workflow, inputs, &making.entry)?;
        let at = now_secs();
        let (mut run, _wf) = Self::new_run(scope, startable, making, at);
        let before = run.steps.clone();
        let effects = run.apply(RunEvent::Start, at)?;
        run.revision = 1;
        self.write_run_snapshot(&run, 0, at)?;
        self.index_run(&run)?;
        let addr = self.run_addr(run.id);
        self.journal_event_of(&run)?;
        self.journal_run_fact(&addr, Self::started_fact(&run), run.id)?;
        self.journal_step_facts(&addr, &run, &before)?;
        if run.is_finished() {
            self.journal_finish_fact(&addr, &run)?;
        }
        Ok((run, effects))
    }

    /// Start a goal's queued run: its turn came. Refused unless the run is
    /// queued (`RunNotQueued`) — a run of the workspace never is — while the
    /// goal has a live run (`RunNotFinished` — which is what makes two
    /// advances of the same queue, or an advance racing a person, harmless:
    /// one starts, the other is refused), and on a closed goal. `Start` is
    /// applied, the fact journaled, and the goal's `run` becomes this run.
    pub fn start_queued_run(
        &self,
        run_id: RunId,
    ) -> Result<(WorkflowRun, Vec<RunEffect>), StoreError> {
        let _one_writer = self.run_writer();
        let mut run = self.get_run(run_id)?;
        let Some(goal_id) = run.scope.goal() else {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-workspace-run-never-queued",
                run = run_id.to_string()
            )));
        };
        let mut goal = self.get_goal(goal_id)?;
        if !run.is_queued() {
            return Err(StoreError::RunNotQueued {
                goal: goal.id.to_string(),
                run: run_id.to_string(),
            });
        }
        if goal.is_closed() {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-goal-closed-nothing-runs-2",
                a0 = (goal.id).to_string()
            )));
        }
        if let Some(live) = self.live_run_of(&goal)? {
            return Err(StoreError::RunNotFinished {
                goal: goal.id.to_string(),
                run: live.id.to_string(),
            });
        }
        let at = now_secs();
        let before = run.steps.clone();
        let effects = run.apply(RunEvent::Start, at)?;
        let expected = run.revision;
        run.revision = expected + 1;
        self.write_run_snapshot(&run, expected, at)?;
        let addr = self.goal_addr(&goal);
        self.journal_event_of(&run)?;
        self.journal_run_fact(&addr, Self::started_fact(&run), run.id)?;
        self.journal_step_facts(&addr, &run, &before)?;
        if run.is_finished() {
            self.journal_finish_fact(&addr, &run)?;
        }
        goal.run = Some(run.id);
        goal.revision += 1;
        self.write_goal_snapshot(&goal, at)?;
        {
            let idx = self.idx();
            idx.in_transaction(|| {
                self.index_run_in(&idx, &run)?;
                self.index_goal_in(&idx, &goal, Some(&run))
            })?;
        }
        Ok((run, effects))
    }

    /// The one way a run changes.
    ///
    /// A `Decided` event must name a Decision the journal holds for exactly
    /// this step (`approval:<run>/<step>`), whose verdict matches and whose
    /// signer passes the governance policy; a mismatch is refused by name. An
    /// `Amended` workflow is validated first. Then `apply` decides, the
    /// snapshot is written at the next revision, one fact per changed step
    /// (and one for the run when it finished) is journaled, and the index
    /// follows.
    ///
    /// Two completions never lose each other: the read, the apply and the
    /// write happen under one lock per process, and the write is a
    /// compare-and-swap on the run's revision. A run that moved under the
    /// write from outside the process (a peer's snapshot) is re-read and the
    /// event re-applied, a bounded number of times.
    pub fn record_run_event(
        &self,
        run_id: RunId,
        event: RunEvent,
    ) -> Result<(WorkflowRun, Vec<RunEffect>), StoreError> {
        let _one_writer = self.run_writer();
        self.record_run_event_locked(run_id, event)
    }

    /// [`Self::record_run_event`] for a caller that already holds the writer.
    fn record_run_event_locked(
        &self,
        run_id: RunId,
        event: RunEvent,
    ) -> Result<(WorkflowRun, Vec<RunEffect>), StoreError> {
        const ATTEMPTS: usize = 3;
        for attempt in 1..=ATTEMPTS {
            match self.record_run_event_once(run_id, event.clone()) {
                Err(StoreError::RevisionConflict { .. }) if attempt < ATTEMPTS => {
                    tracing::debug!(run = %run_id, "the run moved under a write; re-applying");
                }
                other => return other,
            }
        }
        Err(StoreError::Invalid(bisa_core::text!(
            "error-store-invalid-run-kept-moving-under-attempts-record-event",
            run_id = run_id.to_string(),
            attempts = (ATTEMPTS).to_string()
        )))
    }

    fn record_run_event_once(
        &self,
        run_id: RunId,
        event: RunEvent,
    ) -> Result<(WorkflowRun, Vec<RunEffect>), StoreError> {
        let mut run = self.get_run(run_id)?;
        // A goal's run reads its goal: the journal its facts go to, and the
        // row that caches the goal's status. A run of the workspace is its
        // own home and caches nobody's status.
        let goal = match run.scope.goal() {
            Some(goal) => Some(self.get_goal(goal)?),
            None => None,
        };
        let addr = match &goal {
            Some(goal) => self.goal_addr(goal),
            None => self.run_addr(run.id),
        };
        if let RunEvent::Decided {
            step,
            approve,
            approval,
        } = &event
        {
            self.decision_author(
                &addr.home,
                Gate::Approval,
                approval,
                Some(&approval_subject(run_id, step)),
                *approve,
            )?;
        }
        if let RunEvent::Amended { workflow } = &event {
            let problems = self.validate_workflow(workflow)?;
            if !problems.is_empty() {
                return Err(StoreError::WorkflowInvalid(problems));
            }
        }
        let at = now_secs();
        let before = run.steps.clone();
        let amended = matches!(event, RunEvent::Amended { .. });
        let cancelled = match &event {
            RunEvent::Cancel { cause } => Some(cause.clone()),
            _ => None,
        };
        let effects = run.apply(event, at)?;
        let expected = run.revision;
        run.revision = expected + 1;
        self.write_run_snapshot(&run, expected, at)?;
        if amended {
            self.journal_run_fact(
                &addr,
                RunFact::Amended {
                    revision: run.workflow.revision,
                },
                run.id,
            )?;
        }
        self.journal_step_facts(&addr, &run, &before)?;
        if let Some(cause) = cancelled {
            self.journal_run_fact(&addr, RunFact::Cancelled { cause }, run.id)?;
        } else if run.is_finished() {
            self.journal_finish_fact(&addr, &run)?;
        }
        // The projection, as one unit: the run row, its step rows and — for
        // a goal's current run — the goal row that caches the status land
        // together or not at all. The goal snapshot itself is unchanged —
        // status is never written there.
        {
            let idx = self.idx();
            idx.in_transaction(|| {
                self.index_run_in(&idx, &run)?;
                if let Some(goal) = goal.as_ref().filter(|g| g.run == Some(run.id)) {
                    self.index_goal_in(&idx, goal, Some(&run))?;
                }
                Ok(())
            })?;
        }
        Ok((run, effects))
    }

    /// Close a goal: `Cancel { closed }` on every queued run and on the live
    /// one, then `closed = Some(..)`. The one move a goal makes on its own;
    /// each cancelled run comes back with its effects, for the engine to
    /// settle.
    pub fn set_goal_closed(
        &self,
        goal_id: GoalId,
        reason: ClosureReason,
    ) -> Result<(Goal, Vec<CancelledRun>), StoreError> {
        let _one_writer = self.run_writer();
        let mut goal = self.get_goal(goal_id)?;
        if goal.is_closed() {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-goal-already-closed",
                goal_id = goal_id.to_string()
            )));
        }
        let cause = CancelCause::Closed {
            reason: reason.clone(),
        };
        let mut cancelled = Vec::new();
        for queued in self.queued_runs_of(&goal)? {
            cancelled.push(self.record_run_event_locked(
                queued.id,
                RunEvent::Cancel {
                    cause: cause.clone(),
                },
            )?);
        }
        let mut current: Option<WorkflowRun> = None;
        if let Some(id) = goal.run {
            let run = self.get_run(id)?;
            if run.is_live() {
                let (r, e) = self.record_run_event_locked(id, RunEvent::Cancel { cause })?;
                current = Some(r.clone());
                cancelled.push((r, e));
            } else {
                current = Some(run);
            }
        }
        let at = now_secs();
        goal.closed = Some(Closure { reason, at });
        // A closed goal hears nothing more: its listening ends with it, and
        // what its events had queued settles.
        goal.listening = None;
        goal.revision += 1;
        self.write_goal_snapshot(&goal, at)?;
        self.index_goal(&goal, current.as_ref())?;
        let host = bisa_core::ListenerHost::Goal { goal: goal_id };
        self.settle_pending_signals(
            &host,
            crate::signals::SignalState::Skipped,
            "the goal is closed",
        )?;
        self.forget_listener_runtimes(&host)?;
        Ok((goal, cancelled))
    }

    /// Put a closed goal away, or take it back out. The second move a goal
    /// makes on its own, and the one that goes back; an open goal is closed
    /// first by its caller — archiving never cancels anything itself.
    pub fn set_goal_archived(&self, goal_id: GoalId, archived: bool) -> Result<Goal, StoreError> {
        let mut goal = self.get_goal(goal_id)?;
        if archived && !goal.is_closed() {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-goal-open-close-before-archiving",
                goal_id = goal_id.to_string()
            )));
        }
        if goal.is_archived() == archived {
            return Ok(goal);
        }
        let at = now_secs();
        goal.archived = archived.then(|| bisa_core::Archived::at(at));
        goal.revision += 1;
        self.write_goal_snapshot(&goal, at)?;
        self.index_goal(&goal, None)?;
        Ok(goal)
    }

    /// The workflow the next run will use. Refused while a run is live or
    /// queued: a run holds its own copy, and swapping the goal's pointer
    /// under it would make the goal describe one thing and run another.
    pub fn set_goal_workflow(
        &self,
        goal_id: GoalId,
        workflow: Option<WorkflowId>,
    ) -> Result<Goal, StoreError> {
        let _one_writer = self.run_writer();
        let mut goal = self.get_goal(goal_id)?;
        if goal.is_closed() {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-goal-closed",
                goal_id = goal_id.to_string()
            )));
        }
        let run = match goal.run {
            Some(current) => Some(self.get_run(current)?),
            None => None,
        };
        if let Some(busy) = self.busy_run_of(&goal)? {
            return Err(StoreError::RunNotFinished {
                goal: goal_id.to_string(),
                run: busy.to_string(),
            });
        }
        if let Some(id) = workflow {
            let wf = self.get_workflow(id)?;
            if wf.is_archived() {
                return Err(StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-workflow-archived-unarchive-before-pointing-goal",
                    a0 = (wf.name).to_string()
                )));
            }
            if let Some(designed_for) = wf.origin.goal() {
                if designed_for != goal_id {
                    return Err(StoreError::Invalid(bisa_core::text!(
                        "error-store-invalid-workflow-was-designed-goal-promote-library-first",
                        a0 = (wf.name).to_string(),
                        designed_for = designed_for.to_string()
                    )));
                }
            }
        }
        if goal.workflow == workflow {
            return Ok(goal);
        }
        goal.workflow = workflow;
        goal.revision += 1;
        let at = now_secs();
        self.write_goal_snapshot(&goal, at)?;
        self.index_goal(&goal, run.as_ref())?;
        Ok(goal)
    }

    /// A run by its id, read from its home: a run of the workspace from its
    /// own folder, which its id names — no index needed — and a goal's
    /// through the index row that names its goal.
    pub fn get_run(&self, run: RunId) -> Result<WorkflowRun, StoreError> {
        let d = run.to_string();
        if let Some((run, _)) =
            self.snapshots
                .get::<WorkflowRun>(&Paths::ns_run(run), KIND_WORKFLOW_RUN, &d)?
        {
            return Ok(run);
        }
        let goal: GoalId = self
            .idx()
            .goal_of_run(&d)?
            .flatten()
            .and_then(|g| g.parse().ok())
            .ok_or_else(|| StoreError::RunNotFound(d.clone()))?;
        match self
            .snapshots
            .get::<WorkflowRun>(&Paths::ns_goal(goal), KIND_WORKFLOW_RUN, &d)?
        {
            Some((run, _)) => Ok(run),
            None => Err(StoreError::RunNotFound(d)),
        }
    }

    /// The goal's current run, when it has one: the live run, else the
    /// latest that started. Never a queued run.
    pub fn get_current_run(&self, goal: GoalId) -> Result<Option<WorkflowRun>, StoreError> {
        let g = self.get_goal(goal)?;
        match g.run {
            Some(run) => Ok(Some(self.get_run(run)?)),
            None => Ok(None),
        }
    }

    /// The goal's live run — started and unfinished — when it has one.
    pub fn live_run(&self, goal: GoalId) -> Result<Option<WorkflowRun>, StoreError> {
        self.live_run_of(&self.get_goal(goal)?)
    }

    fn live_run_of(&self, goal: &Goal) -> Result<Option<WorkflowRun>, StoreError> {
        match goal.run {
            Some(id) => Ok(Some(self.get_run(id)?).filter(WorkflowRun::is_live)),
            None => Ok(None),
        }
    }

    /// The goal's queued runs, first to start first. The index says which
    /// and in what order; each snapshot is re-read and re-checked, so a row
    /// the index is behind on is never handed out as queued.
    pub fn queued_runs(&self, goal: GoalId) -> Result<Vec<WorkflowRun>, StoreError> {
        self.queued_runs_of(&self.get_goal(goal)?)
    }

    fn queued_runs_of(&self, goal: &Goal) -> Result<Vec<WorkflowRun>, StoreError> {
        let mut out = Vec::new();
        // The rows first, then the loop: a guard alive across the body would
        // meet `get_run`'s own lock (`tests/it/locking.rs`).
        let ids = self.idx().queued_run_ids(&goal.id.to_string())?;
        for id in ids {
            let Ok(run_id) = id.parse::<RunId>() else {
                continue;
            };
            let run = self.get_run(run_id)?;
            if run.is_queued() {
                out.push(run);
            }
        }
        Ok(out)
    }

    /// How many runs wait in the goal's queue — the index's count, for a
    /// list row that must not open every snapshot.
    pub fn queued_run_count(&self, goal: GoalId) -> Result<usize, StoreError> {
        Ok(self.idx().queued_run_ids(&goal.to_string())?.len())
    }

    /// The queued run whose turn is next, when the goal has one.
    pub fn next_queued_run(&self, goal: GoalId) -> Result<Option<WorkflowRun>, StoreError> {
        Ok(self.queued_runs(goal)?.into_iter().next())
    }

    /// Whether the goal has a live or a queued run: nothing else may start
    /// on it, and its workflow may not change.
    pub fn goal_is_busy(&self, goal: &Goal) -> Result<bool, StoreError> {
        Ok(self.busy_run_of(goal)?.is_some())
    }

    /// The run that makes the goal busy: the live one, else the first queued.
    fn busy_run_of(&self, goal: &Goal) -> Result<Option<RunId>, StoreError> {
        if let Some(live) = self.live_run_of(goal)? {
            return Ok(Some(live.id));
        }
        Ok(self.queued_runs_of(goal)?.first().map(|r| r.id))
    }

    /// The runs of the workspace — of one workflow, or of all — as the index
    /// lists them, each re-read from its snapshot: `live` keeps the started
    /// and unfinished ones alone, re-checked against the snapshot, so a row
    /// the index is behind on is never handed out as live. A run this build
    /// cannot read is skipped and said (`tolerated`).
    pub fn workspace_runs(
        &self,
        workflow: Option<WorkflowId>,
        live: bool,
    ) -> Result<Vec<WorkflowRun>, StoreError> {
        // The rows first, then the loop: a guard alive across the body would
        // meet `get_run`'s own lock (`tests/it/locking.rs`).
        let ids = self
            .idx()
            .workspace_run_ids(workflow.map(|w| w.to_string()).as_deref(), live)?;
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            let Ok(run_id) = id.parse::<RunId>() else {
                continue;
            };
            let read =
                self.snapshots
                    .get::<WorkflowRun>(&Paths::ns_run(run_id), KIND_WORKFLOW_RUN, &id);
            if let Some((run, _)) = crate::workspace::tolerated("run", &id, read)?.flatten() {
                if !live || run.is_live() {
                    out.push(run);
                }
            }
        }
        Ok(out)
    }

    /// Every run of the workspace a workflow has, in the order they were
    /// made, oldest first — the workflow's own run history.
    pub fn list_workflow_runs(&self, workflow: WorkflowId) -> Result<Vec<WorkflowRun>, StoreError> {
        self.workspace_runs(Some(workflow), false)
    }

    /// The runs of the workspace that are going — of one workflow, or of
    /// every one (the restart walk's).
    pub fn live_workspace_runs(
        &self,
        workflow: Option<WorkflowId>,
    ) -> Result<Vec<WorkflowRun>, StoreError> {
        self.workspace_runs(workflow, true)
    }

    /// Every run of the workspace's folder on disk, by id — what a rebuild
    /// walks, whatever the index says.
    pub(crate) fn workspace_run_ids_on_disk(&self) -> Result<Vec<RunId>, StoreError> {
        let dir = self.paths.workspace_runs_dir();
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(StoreError::io(dir.display().to_string(), e)),
        };
        let mut out = Vec::new();
        for entry in entries.flatten() {
            if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            match entry.file_name().to_string_lossy().parse::<RunId>() {
                Ok(id) => out.push(id),
                Err(_) => tracing::warn!("{}: not a run id, skipping", entry.path().display()),
            }
        }
        out.sort();
        Ok(out)
    }

    /// Forget a workflow's runs of the workspace, folders and rows — what a
    /// deleted workflow takes with it. Refused, before anything goes, while
    /// one of them is live: the engine retires them first.
    pub(crate) fn delete_workflow_runs(&self, workflow: &Workflow) -> Result<(), StoreError> {
        let runs = self.list_workflow_runs(workflow.id)?;
        if let Some(live) = runs.iter().find(|r| !r.is_finished()) {
            return Err(StoreError::StillUsed(bisa_core::text!(
                "error-store-still-used-workflow-has-run-going",
                a0 = (workflow.name).to_string(),
                run = live.id.to_string()
            )));
        }
        for run in runs {
            self.forget_workspace_run(&run)?;
        }
        Ok(())
    }

    /// How many finished runs of the workspace a workflow keeps —
    /// `workflow.runs.keep`, the workspace's or this machine's word, never
    /// below one.
    pub fn workspace_runs_kept(&self) -> Result<usize, StoreError> {
        let n = self
            .setting(WorkflowRun::SETTING_KEPT, None)?
            .value
            .as_u64()
            .unwrap_or(0);
        Ok(usize::try_from(n).unwrap_or(usize::MAX).max(1))
    }

    /// Put away a workflow's oldest finished runs of the workspace beyond
    /// `keep` — folder and rows, oldest first — and say which went. The bound
    /// on a history a schedule grows daily: live and queued runs are never
    /// counted and never touched, the newest `keep` finished ones stay, and
    /// a goal's runs are the goal's. A run this build cannot read is left
    /// where it is: nothing is put away unread.
    pub fn forget_finished_workspace_runs_beyond(
        &self,
        workflow: WorkflowId,
        keep: usize,
    ) -> Result<Vec<RunId>, StoreError> {
        let finished: Vec<WorkflowRun> = self
            .list_workflow_runs(workflow)?
            .into_iter()
            .filter(WorkflowRun::is_finished)
            .collect();
        let beyond = finished.len().saturating_sub(keep.max(1));
        let mut gone = Vec::with_capacity(beyond);
        for run in finished.into_iter().take(beyond) {
            self.forget_workspace_run(&run)?;
            gone.push(run.id);
        }
        Ok(gone)
    }

    /// One run of the workspace put away: its folder, then its rows (the
    /// steps, the work items, the decisions and the spend cascade).
    fn forget_workspace_run(&self, run: &WorkflowRun) -> Result<(), StoreError> {
        let home = self.paths.home(&run.home());
        match std::fs::remove_dir_all(home.dir()) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(StoreError::io(home.dir().display().to_string(), e)),
        }
        self.idx().delete_workspace_run(&run.id.to_string())
    }

    /// Every run of a goal in queue order, oldest first — the queued ones at
    /// the tail. A run file this build cannot read is skipped and said at
    /// `error` (`tolerated`): the goal's history is a list, and one broken
    /// file must not empty it.
    pub fn list_runs(&self, goal: GoalId) -> Result<Vec<WorkflowRun>, StoreError> {
        let ns = Paths::ns_goal(goal);
        let mut out = Vec::new();
        for d in self.snapshots.list_ds(&ns, KIND_WORKFLOW_RUN)? {
            let read = self
                .snapshots
                .get::<WorkflowRun>(&ns, KIND_WORKFLOW_RUN, &d);
            if let Some((run, _)) = crate::workspace::tolerated("run", &d, read)?.flatten() {
                out.push(run);
            }
        }
        out.sort_by_key(|r| (r.queued_at, r.id));
        Ok(out)
    }

    /// Armed `wait` steps: `(run, step, what it waits for)`, every one.
    pub fn list_armed_waits(&self) -> Result<Vec<(RunId, StepId, WaitFor)>, StoreError> {
        let rows = self.idx().armed_steps()?;
        self.waits_of(rows)
    }

    /// Armed timers and schedules due at or before `now`.
    pub fn list_due_waits(&self, now: u64) -> Result<Vec<(RunId, StepId)>, StoreError> {
        Ok(self
            .idx()
            .due_steps(now)?
            .into_iter()
            .filter_map(|r| Some((r.run_id.parse().ok()?, StepId::new(&r.step_id).ok()?)))
            .collect())
    }

    fn waits_of(&self, rows: Vec<RunStepRow>) -> Result<Vec<(RunId, StepId, WaitFor)>, StoreError> {
        let mut out = Vec::new();
        for row in rows {
            let (Ok(run_id), Ok(step_id)) =
                (row.run_id.parse::<RunId>(), StepId::new(&row.step_id))
            else {
                continue;
            };
            let run = self.get_run(run_id)?;
            if let Some(StepKind::Wait { until }) = run.workflow.step(&step_id).map(|s| &s.kind) {
                out.push((run_id, step_id, until.clone()));
            }
        }
        Ok(out)
    }

    /// Write the run at `run.revision`, which must be `expected + 1` — the
    /// snapshot store refuses when the stored copy is not at `expected`.
    fn write_run_snapshot(
        &self,
        run: &WorkflowRun,
        expected: u64,
        at: u64,
    ) -> Result<(), StoreError> {
        let d = run.id.to_string();
        let home = run.home();
        let event = self.snapshots.put_expecting(
            &Paths::ns_home(&home),
            KIND_WORKFLOW_RUN,
            &d,
            "run",
            run,
            expected,
            &self.owner,
            at,
            None,
            &[],
        )?;
        self.emit_store_event(StoreEvent::SnapshotWritten {
            kind: KIND_WORKFLOW_RUN,
            home,
            event,
        });
        Ok(())
    }

    fn journal_run_fact(
        &self,
        addr: &JournalAddr,
        fact: RunFact,
        run: RunId,
    ) -> Result<(), StoreError> {
        self.journal_owner_fact(addr, JournalPayload::Run { run, event: fact })
    }

    fn journal_finish_fact(&self, addr: &JournalAddr, run: &WorkflowRun) -> Result<(), StoreError> {
        if let Some(outcome) = run.outcome {
            self.journal_run_fact(addr, RunFact::Finished { outcome }, run.id)?;
        }
        Ok(())
    }

    /// One fact per record whose state changed — or whose work item did: a
    /// `StepStarted` moves neither state nor seq, and the item the step runs
    /// on is the fact a restart reads — in the workflow's step order. A
    /// boundary event that acted beside a live step changes no state; its
    /// fire is the fact.
    fn journal_step_facts(
        &self,
        addr: &JournalAddr,
        run: &WorkflowRun,
        before: &BTreeMap<StepId, StepRecord>,
    ) -> Result<(), StoreError> {
        for step in &run.workflow.steps {
            let Some(after) = run.steps.get(&step.id) else {
                continue;
            };
            let earlier = before.get(&step.id);
            let changed = match earlier {
                Some(b) => {
                    b.state != after.state || b.seq != after.seq || b.work_item != after.work_item
                }
                None => after.state != StepState::Pending,
            };
            let mut facts = Vec::new();
            if changed && after.state != StepState::Pending {
                facts.push(step_fact(after));
            } else if let Some(b) = earlier {
                for (name, fired) in &after.fired {
                    if b.fired.get(name).map(|f| f.count) != Some(fired.count) {
                        facts.push(StepFact::Boundary {
                            boundary: name.clone(),
                        });
                    }
                }
            }
            for fact in facts {
                self.journal_owner_fact(
                    addr,
                    JournalPayload::Step {
                        run: run.id,
                        step: step.id.clone(),
                        event: fact,
                    },
                )?;
            }
        }
        Ok(())
    }

    /// A fact the platform writes on a run's home journal, signed as the
    /// owner.
    fn journal_owner_fact(
        &self,
        addr: &JournalAddr,
        payload: JournalPayload,
    ) -> Result<(), StoreError> {
        let searchable = crate::workspace::searchable_text(&payload);
        let je = JournalEvent {
            home: addr.home,
            author: self.owner_principal(),
            at: now_secs(),
            payload,
        };
        let event = self
            .log
            .append(addr, &je, &self.owner, &self.owner.public_key(), None)?;
        self.emit_store_event(StoreEvent::JournalAppended {
            home: addr.home,
            event: event.clone(),
        });
        // The same seams every appended journal event passes: the search
        // index (a goal's facts — search finds goals) and the activity feed —
        // a run's facts are rows of the pulse.
        if let (Some(text), Some(goal)) = (searchable, addr.home.goal()) {
            self.idx()
                .index_text(&event.id.to_hex(), &goal.to_string(), &text)?;
        }
        self.record_journal_activity(&je)?;
        Ok(())
    }

    pub(crate) fn index_run(&self, run: &WorkflowRun) -> Result<(), StoreError> {
        let idx = self.idx();
        idx.in_transaction(|| self.index_run_in(&idx, run))
    }

    /// The run's projection on an index the caller already holds — inside
    /// the caller's transaction when it has one.
    pub(crate) fn index_run_in(
        &self,
        idx: &crate::index::Index,
        run: &WorkflowRun,
    ) -> Result<(), StoreError> {
        let d = run.id.to_string();
        let now = now_secs();
        let rows: Vec<RunStepRow> = run
            .workflow
            .steps
            .iter()
            .filter_map(|step| {
                let record = run.steps.get(&step.id)?;
                Some(RunStepRow {
                    run_id: d.clone(),
                    step_id: step.id.to_string(),
                    kind: step.kind.as_str().to_string(),
                    state: record.state.as_str().to_string(),
                    wait_topic: wait_word(&step.kind, &record.state),
                    due_at: None,
                    work_item: record.work_item.map(|w| w.to_string()),
                    updated_at: now,
                })
            })
            .collect();
        idx.upsert_run(&run_row(run))?;
        idx.replace_run_steps(&d, &rows)
    }

    /// Record when a timer or schedule step comes due, so the ticker finds it
    /// without opening every run. The snapshot is untouched: a due time is
    /// this node's bookkeeping, derived again on re-arm.
    pub fn set_wait_due(
        &self,
        run: RunId,
        step: &StepId,
        due_at: Option<u64>,
    ) -> Result<(), StoreError> {
        let r = self.get_run(run)?;
        let mut rows = self.idx().waiting_steps(None)?;
        rows.retain(|row| row.run_id == run.to_string());
        // Re-derive every row for the run, with the one due time changed.
        let d = run.to_string();
        let now = now_secs();
        let rows: Vec<RunStepRow> = r
            .workflow
            .steps
            .iter()
            .filter_map(|s| {
                let record = r.steps.get(&s.id)?;
                let wait_topic = wait_word(&s.kind, &record.state);
                let existing_due = rows
                    .iter()
                    .find(|row| row.step_id == s.id.as_str())
                    .and_then(|row| row.due_at);
                Some(RunStepRow {
                    run_id: d.clone(),
                    step_id: s.id.to_string(),
                    kind: s.kind.as_str().to_string(),
                    state: record.state.as_str().to_string(),
                    wait_topic,
                    due_at: if &s.id == step { due_at } else { existing_due },
                    work_item: record.work_item.map(|w| w.to_string()),
                    updated_at: now,
                })
            })
            .collect();
        self.idx().replace_run_steps(&d, &rows)
    }

    /// Rebuild support: every run snapshot of a goal, with its steps.
    pub(crate) fn reindex_runs_of(&self, goal: GoalId) -> Result<(), StoreError> {
        for run in self.list_runs(goal)? {
            self.index_run(&run)?;
        }
        Ok(())
    }

    /// A run of the workspace's snapshot, straight from its folder — never
    /// through the index, which the rebuild and the reconcile exist to write.
    pub(crate) fn workspace_run_snapshot(
        &self,
        run: RunId,
    ) -> Result<Option<WorkflowRun>, StoreError> {
        let d = run.to_string();
        let read = self
            .snapshots
            .get::<WorkflowRun>(&Paths::ns_run(run), KIND_WORKFLOW_RUN, &d);
        Ok(crate::workspace::tolerated("run", &d, read)?
            .flatten()
            .map(|(run, _)| run))
    }
}

/// The run's locator row. A run a queued signal began names the listener it
/// came from — what that listener's guard counts — and the signal.
fn run_row(run: &WorkflowRun) -> RunRow {
    let listener = run
        .dispatched
        .as_ref()
        .and(run.event.as_ref())
        .and_then(|e| e.listener.as_ref())
        .map(ToString::to_string);
    RunRow {
        id: run.id.to_string(),
        scope: run.scope.as_str().to_string(),
        goal_id: run.scope.goal().map(|g| g.to_string()),
        workflow_id: run.workflow.id.to_string(),
        status: run.status().as_str().to_string(),
        revision: run.revision,
        queued_at: run.queued_at,
        started_at: run.started_at,
        finished_at: run.finished_at,
        dispatched: listener.as_ref().and(run.dispatched.clone()),
        listener,
    }
}

/// What a waiting `wait` holds for, as the index names it: `signal:<name>`,
/// `message`, `project`, `run`, `platform:<topic>`, `release`. A timer is
/// found by its due time instead; anything not waiting names nothing.
fn wait_word(kind: &StepKind, state: &StepState) -> Option<String> {
    let (StepKind::Wait { until }, StepState::Waiting) = (kind, state) else {
        return None;
    };
    Some(match until {
        WaitFor::Signal { filter } => format!("signal:{}", filter.name),
        WaitFor::Platform { filter } => format!("platform:{}", filter.topic),
        WaitFor::Message { .. }
        | WaitFor::Project { .. }
        | WaitFor::Run { .. }
        | WaitFor::Release => until.as_str().to_string(),
        WaitFor::Delay { .. } | WaitFor::Time { .. } | WaitFor::Schedule { .. } => return None,
    })
}

/// The fact a record's state reads as.
fn step_fact(record: &StepRecord) -> StepFact {
    match &record.state {
        StepState::Pending | StepState::Running => StepFact::Started {
            work_item: record.work_item,
        },
        StepState::Waiting => StepFact::Waiting,
        StepState::Done { branches } => match (&record.answer, &record.gate) {
            (Some(answer), _) => StepFact::Answered {
                answer: answer.clone(),
            },
            (None, Some(gate)) => StepFact::Decided {
                approve: true,
                approval: bisa_core::ApprovalId(gate.clone()),
            },
            (None, None) => StepFact::Done {
                branches: branches.clone(),
            },
        },
        StepState::Diverted { by } => StepFact::Diverted { by: by.clone() },
        StepState::Failed => match &record.gate {
            Some(gate) => StepFact::Decided {
                approve: false,
                approval: bisa_core::ApprovalId(gate.clone()),
            },
            None => StepFact::Failed {
                error: record.error.clone().unwrap_or_default(),
            },
        },
        StepState::Skipped => StepFact::Skipped,
        StepState::Cancelled => StepFact::Cancelled,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::MemoryKeyStore;
    use crate::workflows::tests::{agent_step, notify_workflow, sid, step};
    use crate::workflows::NewWorkflow;
    use crate::workspace::NewGoal;
    use bisa_core::{
        Answer, AskOption, Home, InputDef, InputKind, RunOutcome, RunStatus, Tags, WorkItemSpec,
        WorkItemState,
    };
    use serde_json::json;

    /// A run of `goal`.
    fn on_goal(goal: GoalId) -> RunScope {
        RunScope::Goal { goal }
    }

    /// A goal's home.
    fn home_of(goal: GoalId) -> Home {
        Home::Goal { goal }
    }

    fn ws() -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        let w =
            Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
        (dir, w)
    }

    fn staffed(ws: &Workspace) {
        ws.add_agent(crate::agents::NewAgent {
            name: "Developer".into(),
            harness: "mock".into(),
            system_prompt: "build".into(),
            ..Default::default()
        })
        .unwrap();
    }

    /// agent → approval → end.
    fn approval_workflow() -> NewWorkflow {
        NewWorkflow {
            name: "Gated".into(),
            description: String::new(),
            inputs: vec![],
            steps: vec![
                agent_step("build", "developer", &["ship"]),
                step(
                    "ship",
                    StepKind::Approval {
                        prompt: "Ship?".into(),
                    },
                    &["end"],
                ),
                step(
                    "end",
                    StepKind::End {
                        finish: bisa_core::Finish::Done,
                    },
                    &[],
                ),
            ],
            tags: Tags::default(),
            decision_making: false,
        }
    }

    /// A goal's run history is a list: a run file this build cannot read is
    /// skipped and said, never the reason the history is empty or the read
    /// fails.
    #[test]
    fn list_runs_skips_an_unreadable_run_snapshot() {
        let (_dir, ws) = ws();
        staffed(&ws);
        let wf = ws
            .create_workflow(approval_workflow(), bisa_core::WorkflowOrigin::Workspace)
            .unwrap();
        let goal = ws.create_goal(NewGoal::captured("keep history")).unwrap();
        let (run, _) = ws
            .create_run(
                on_goal(goal.id),
                wf.id,
                BTreeMap::new(),
                bisa_core::RunEntry::by_hand(),
                None,
            )
            .unwrap();
        assert_eq!(ws.list_runs(goal.id).unwrap().len(), 1);

        ws.snapshots
            .put(
                &Paths::ns_goal(goal.id),
                KIND_WORKFLOW_RUN,
                &run.id.to_string(),
                &serde_json::json!({"id": run.id.to_string(), "from_another_build": true}),
                99,
                &ws.owner,
                crate::workspace::now_secs(),
                None,
                &[],
            )
            .unwrap();
        assert!(
            matches!(ws.get_run(run.id), Err(StoreError::Unreadable { .. })),
            "a single read refuses it"
        );
        assert!(
            ws.list_runs(goal.id).unwrap().is_empty(),
            "the list skips it"
        );
    }

    #[test]
    fn a_run_walks_through_the_store() {
        let (_dir, ws) = ws();
        staffed(&ws);
        let wf = ws
            .create_workflow(approval_workflow(), bisa_core::WorkflowOrigin::Workspace)
            .unwrap();
        let goal = ws.create_goal(NewGoal::captured("build a parser")).unwrap();
        assert_eq!(goal.status(None), bisa_core::GoalStatus::Draft);

        let (run, effects) = ws
            .create_run(
                on_goal(goal.id),
                wf.id,
                BTreeMap::new(),
                bisa_core::RunEntry::by_hand(),
                None,
            )
            .unwrap();
        assert_eq!(effects, vec![RunEffect::StartAgent { step: sid("build") }]);
        assert_eq!(run.revision, 1);
        let g = ws.get_goal(goal.id).unwrap();
        assert_eq!(g.run, Some(run.id));
        assert_eq!(g.runs, vec![run.id]);
        assert_eq!(g.workflow, Some(wf.id));
        assert_eq!(
            ws.dump_goal_rows().unwrap()[0].status,
            "running",
            "the row caches the projection"
        );

        // The engine binds a work item and settles it.
        let item = WorkItemSpec {
            id: bisa_core::WorkItemId::from_ulid(mint_ulid()),
            home: home_of(goal.id),
            run: Some(run.id),
            step: Some(sid("build")),
            instructions: "build".into(),
            state: WorkItemState::Open,
            project: None,
            harness_candidates: vec!["mock".into()],
            model: None,
            effort: None,
            output_schema: None,
            budget: Default::default(),
            assignees: vec![],
            tier_ceiling: bisa_core::ToolTier::Write,
            agent: None,
            spawn_allowlist: vec![],
            depth_budget: 0,
            result_attempts: 0,
            interruptions: 0,
        };
        ws.put_work_item(&item).unwrap();
        ws.record_run_event(
            run.id,
            RunEvent::StepStarted {
                step: sid("build"),
                work_item: Some(item.id),
            },
        )
        .unwrap();
        let (run2, effects) = ws
            .record_run_event(
                run.id,
                RunEvent::StepDone {
                    step: sid("build"),
                    output: json!({"summary": "ok"}),
                },
            )
            .unwrap();
        assert_eq!(effects, vec![RunEffect::OpenGate { step: sid("ship") }]);
        assert_eq!(run2.revision, 3);
        assert_eq!(run2.status(), RunStatus::Waiting);
        assert_eq!(ws.dump_goal_rows().unwrap()[0].status, "waiting");
        assert_eq!(
            ws.idx().work_items_for(&goal.id.to_string()).unwrap()[0]
                .step_id
                .as_deref(),
            Some("build")
        );

        // A bogus approval, a wrong-subject approval, a decline that claims to
        // approve: refused by name, run untouched.
        let bogus = bisa_core::ApprovalId("deadbeef".into());
        assert!(matches!(
            ws.record_run_event(
                run.id,
                RunEvent::Decided {
                    step: sid("ship"),
                    approve: true,
                    approval: bogus
                }
            ),
            Err(StoreError::GateDecisionInvalid(_))
        ));
        let wrong = ws
            .record_decision(
                &home_of(goal.id),
                Gate::Approval,
                true,
                "adopt:x",
                None,
                None,
            )
            .unwrap();
        assert!(matches!(
            ws.record_run_event(
                run.id,
                RunEvent::Decided {
                    step: sid("ship"),
                    approve: true,
                    approval: wrong
                }
            ),
            Err(StoreError::GateDecisionInvalid(_))
        ));
        let deny = ws
            .record_decision(
                &home_of(goal.id),
                Gate::Approval,
                false,
                &approval_subject(run.id, &sid("ship")),
                None,
                None,
            )
            .unwrap();
        assert!(matches!(
            ws.record_run_event(
                run.id,
                RunEvent::Decided {
                    step: sid("ship"),
                    approve: true,
                    approval: deny.clone()
                }
            ),
            Err(StoreError::GateDecisionInvalid(_))
        ));
        assert_eq!(ws.get_run(run.id).unwrap().revision, 3, "nothing moved");

        // The decline, recorded as what it is, fails the step and the run.
        let (failed, effects) = ws
            .record_run_event(
                run.id,
                RunEvent::Decided {
                    step: sid("ship"),
                    approve: false,
                    approval: deny,
                },
            )
            .unwrap();
        assert_eq!(failed.status(), RunStatus::Failed);
        assert_eq!(
            effects,
            vec![RunEffect::Finished {
                outcome: RunOutcome::Failed
            }]
        );
        assert_eq!(ws.dump_goal_rows().unwrap()[0].status, "failed");
        assert_eq!(
            ws.get_goal(goal.id)
                .unwrap()
                .status(Some(&ws.get_current_run(goal.id).unwrap().unwrap())),
            bisa_core::GoalStatus::Failed
        );

        // The journal names every move.
        let facts: Vec<String> = ws
            .journal(&home_of(goal.id))
            .unwrap()
            .into_iter()
            .filter_map(|je| match je.payload {
                JournalPayload::Step { step, event, .. } => {
                    Some(format!("{step}:{}", event.as_str()))
                }
                JournalPayload::Run { event, .. } => Some(format!("run:{}", event.as_str())),
                _ => None,
            })
            .collect();
        assert_eq!(
            facts,
            // One fact per state change — and one more when the item a step
            // runs on is bound: `Started` on entry, `Started { work_item }`
            // once the item exists, the fact a restart reads to resume it.
            vec![
                "run:started",
                "build:started",
                "build:started",
                "build:done",
                "ship:waiting",
                "ship:decided",
                "end:cancelled",
                "run:finished",
            ]
        );
    }

    /// The journal's run facts, in order.
    fn run_facts(ws: &Workspace, goal: GoalId) -> Vec<String> {
        ws.journal(&home_of(goal))
            .unwrap()
            .into_iter()
            .filter_map(|je| match je.payload {
                JournalPayload::Run { run, event } => Some(format!("{run}:{}", event.as_str())),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_second_run_queues_behind_the_first() {
        let (_dir, ws) = ws();
        staffed(&ws);
        let wf = ws
            .create_workflow(approval_workflow(), bisa_core::WorkflowOrigin::Workspace)
            .unwrap();
        let goal = ws.create_goal(NewGoal::captured("twice")).unwrap();
        let (first, _) = ws
            .create_run(
                on_goal(goal.id),
                wf.id,
                BTreeMap::new(),
                bisa_core::RunEntry::by_hand(),
                None,
            )
            .unwrap();
        let (second, effects) = ws
            .create_run(
                on_goal(goal.id),
                wf.id,
                BTreeMap::new(),
                bisa_core::RunEntry::by_hand(),
                None,
            )
            .unwrap();
        assert!(effects.is_empty(), "a queued run has no effects yet");
        assert_eq!(second.status(), RunStatus::Queued);
        assert_eq!(second.started_at, None);
        assert_eq!(second.revision, 1);
        let g = ws.get_goal(goal.id).unwrap();
        assert_eq!(g.run, Some(first.id), "the current run is the live one");
        assert_eq!(g.runs, vec![first.id, second.id]);
        assert_eq!(ws.dump_goal_rows().unwrap()[0].status, "running");
        assert_eq!(
            ws.idx()
                .get_run(&second.id.to_string())
                .unwrap()
                .unwrap()
                .status,
            "queued"
        );
        assert_eq!(ws.live_run(goal.id).unwrap().unwrap().id, first.id);
        assert_eq!(
            ws.queued_runs(goal.id)
                .unwrap()
                .iter()
                .map(|r| r.id)
                .collect::<Vec<_>>(),
            vec![second.id]
        );
        assert_eq!(ws.next_queued_run(goal.id).unwrap().unwrap().id, second.id);
        assert!(ws.goal_is_busy(&g).unwrap());
        assert!(matches!(
            ws.set_goal_workflow(goal.id, None),
            Err(StoreError::RunNotFinished { .. })
        ));
        assert!(
            matches!(
                ws.start_queued_run(second.id),
                Err(StoreError::RunNotFinished { .. })
            ),
            "its turn has not come"
        );
        assert!(
            matches!(
                ws.start_queued_run(first.id),
                Err(StoreError::RunNotQueued { .. })
            ),
            "a live run is not started from the queue"
        );

        // The first ends; the second is started from the queue.
        ws.record_run_event(
            first.id,
            RunEvent::Cancel {
                cause: CancelCause::Stopped { rationale: None },
            },
        )
        .unwrap();
        assert_eq!(ws.live_run(goal.id).unwrap(), None);
        assert!(ws.goal_is_busy(&ws.get_goal(goal.id).unwrap()).unwrap());
        let (started, effects) = ws.start_queued_run(second.id).unwrap();
        assert_eq!(effects, vec![RunEffect::StartAgent { step: sid("build") }]);
        assert_eq!(started.status(), RunStatus::Running);
        assert_eq!(started.revision, 2);
        assert!(started.started_at.is_some());
        let g = ws.get_goal(goal.id).unwrap();
        assert_eq!(g.run, Some(second.id));
        assert_eq!(ws.dump_goal_rows().unwrap()[0].status, "running");
        assert!(ws.queued_runs(goal.id).unwrap().is_empty());
        assert!(matches!(
            ws.start_queued_run(second.id),
            Err(StoreError::RunNotQueued { .. })
        ));
        assert_eq!(
            run_facts(&ws, goal.id),
            vec![
                format!("{}:started", first.id),
                format!("{}:queued", second.id),
                format!("{}:cancelled", first.id),
                format!("{}:started", second.id),
            ]
        );
        let listed: Vec<RunId> = ws
            .list_runs(goal.id)
            .unwrap()
            .iter()
            .map(|r| r.id)
            .collect();
        assert_eq!(listed, vec![first.id, second.id], "queue order");
    }

    #[test]
    fn a_new_run_on_a_goal_with_nothing_live_starts_ahead_of_the_queue() {
        let (_dir, ws) = ws();
        staffed(&ws);
        let wf = ws
            .create_workflow(approval_workflow(), bisa_core::WorkflowOrigin::Workspace)
            .unwrap();
        let goal = ws.create_goal(NewGoal::captured("restart")).unwrap();
        let (first, _) = ws
            .create_run(
                on_goal(goal.id),
                wf.id,
                BTreeMap::new(),
                bisa_core::RunEntry::by_hand(),
                None,
            )
            .unwrap();
        let (queued, _) = ws
            .create_run(
                on_goal(goal.id),
                wf.id,
                BTreeMap::new(),
                bisa_core::RunEntry::by_hand(),
                None,
            )
            .unwrap();
        ws.record_run_event(
            first.id,
            RunEvent::Cancel {
                cause: CancelCause::Restarted,
            },
        )
        .unwrap();
        // The restart's replacement: started now, the queue behind it.
        let (replacement, effects) = ws
            .create_run(
                on_goal(goal.id),
                wf.id,
                BTreeMap::new(),
                bisa_core::RunEntry::by_hand(),
                None,
            )
            .unwrap();
        assert_eq!(effects, vec![RunEffect::StartAgent { step: sid("build") }]);
        assert_eq!(replacement.status(), RunStatus::Running);
        assert_eq!(ws.get_goal(goal.id).unwrap().run, Some(replacement.id));
        assert_eq!(
            ws.queued_runs(goal.id)
                .unwrap()
                .iter()
                .map(|r| r.id)
                .collect::<Vec<_>>(),
            vec![queued.id],
            "the queue keeps its place"
        );
        assert!(matches!(
            ws.start_queued_run(queued.id),
            Err(StoreError::RunNotFinished { .. })
        ));
    }

    #[test]
    fn a_queued_run_must_be_of_the_goals_workflow() {
        let (_dir, ws) = ws();
        staffed(&ws);
        let wf = ws
            .create_workflow(approval_workflow(), bisa_core::WorkflowOrigin::Workspace)
            .unwrap();
        let other = ws
            .create_workflow(
                notify_workflow("Other"),
                bisa_core::WorkflowOrigin::Workspace,
            )
            .unwrap();
        let goal = ws.create_goal(NewGoal::captured("one workflow")).unwrap();
        ws.create_run(
            on_goal(goal.id),
            wf.id,
            BTreeMap::new(),
            bisa_core::RunEntry::by_hand(),
            None,
        )
        .unwrap();
        let err = ws
            .create_run(
                on_goal(goal.id),
                other.id,
                BTreeMap::new(),
                bisa_core::RunEntry::by_hand(),
                None,
            )
            .unwrap_err();
        assert!(err.is_refusal());
        assert!(
            err.to_string().contains("must be of that workflow"),
            "{err}"
        );
        assert_eq!(
            ws.get_goal(goal.id).unwrap().runs.len(),
            1,
            "nothing written"
        );
        assert_eq!(ws.list_runs(goal.id).unwrap().len(), 1);
    }

    #[test]
    fn a_cancelled_run_leaves_the_goal_ready() {
        let (_dir, ws) = ws();
        staffed(&ws);
        let wf = ws
            .create_workflow(approval_workflow(), bisa_core::WorkflowOrigin::Workspace)
            .unwrap();
        let goal = ws.create_goal(NewGoal::captured("again")).unwrap();
        let (run, _) = ws
            .create_run(
                on_goal(goal.id),
                wf.id,
                BTreeMap::new(),
                bisa_core::RunEntry::by_hand(),
                None,
            )
            .unwrap();
        let (cancelled, effects) = ws
            .record_run_event(
                run.id,
                RunEvent::Cancel {
                    cause: CancelCause::Stopped {
                        rationale: Some("enough".into()),
                    },
                },
            )
            .unwrap();
        assert_eq!(
            effects,
            vec![
                RunEffect::CancelWork {
                    steps: vec![sid("build")]
                },
                RunEffect::Cancelled {
                    cause: CancelCause::Stopped {
                        rationale: Some("enough".into())
                    }
                }
            ]
        );
        assert_eq!(cancelled.status(), RunStatus::Cancelled);
        assert_eq!(
            ws.get_goal(goal.id).unwrap().status(Some(&cancelled)),
            bisa_core::GoalStatus::Draft,
            "a cancelled run leaves the goal ready"
        );
        assert_eq!(ws.dump_goal_rows().unwrap()[0].status, "draft");
        assert!(!ws.goal_is_busy(&ws.get_goal(goal.id).unwrap()).unwrap());
        ws.set_goal_workflow(goal.id, None).unwrap();
        ws.set_goal_workflow(goal.id, Some(wf.id)).unwrap();
        let (second, effects) = ws
            .create_run(
                on_goal(goal.id),
                wf.id,
                BTreeMap::new(),
                bisa_core::RunEntry::by_hand(),
                None,
            )
            .unwrap();
        assert_eq!(
            effects,
            vec![RunEffect::StartAgent { step: sid("build") }],
            "an idle goal starts the run at once"
        );
        assert_ne!(second.id, run.id);
        assert_eq!(ws.get_goal(goal.id).unwrap().run, Some(second.id));
        assert_eq!(ws.get_goal(goal.id).unwrap().runs, vec![run.id, second.id]);
        assert_eq!(
            run_facts(&ws, goal.id),
            vec![
                format!("{}:started", run.id),
                format!("{}:cancelled", run.id),
                format!("{}:started", second.id),
            ]
        );
        let cancelled_fact = ws
            .journal(&home_of(goal.id))
            .unwrap()
            .into_iter()
            .find_map(|je| match je.payload {
                JournalPayload::Run {
                    event: RunFact::Cancelled { cause },
                    ..
                } => Some(cause),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            cancelled_fact,
            CancelCause::Stopped {
                rationale: Some("enough".into())
            }
        );
    }

    #[test]
    fn withdrawing_a_queued_run_keeps_the_rest_in_order() {
        let (_dir, ws) = ws();
        staffed(&ws);
        let wf = ws
            .create_workflow(approval_workflow(), bisa_core::WorkflowOrigin::Workspace)
            .unwrap();
        let goal = ws.create_goal(NewGoal::captured("three")).unwrap();
        let (live, _) = ws
            .create_run(
                on_goal(goal.id),
                wf.id,
                BTreeMap::new(),
                bisa_core::RunEntry::by_hand(),
                None,
            )
            .unwrap();
        let (q1, _) = ws
            .create_run(
                on_goal(goal.id),
                wf.id,
                BTreeMap::new(),
                bisa_core::RunEntry::by_hand(),
                None,
            )
            .unwrap();
        let (q2, _) = ws
            .create_run(
                on_goal(goal.id),
                wf.id,
                BTreeMap::new(),
                bisa_core::RunEntry::by_hand(),
                None,
            )
            .unwrap();
        let ids = |ws: &Workspace| -> Vec<RunId> {
            ws.queued_runs(goal.id)
                .unwrap()
                .iter()
                .map(|r| r.id)
                .collect()
        };
        assert_eq!(ids(&ws), vec![q1.id, q2.id]);
        let (withdrawn, effects) = ws
            .record_run_event(
                q1.id,
                RunEvent::Cancel {
                    cause: CancelCause::Withdrawn,
                },
            )
            .unwrap();
        assert_eq!(
            effects,
            vec![RunEffect::Cancelled {
                cause: CancelCause::Withdrawn
            }],
            "nothing was live in a queued run"
        );
        assert_eq!(withdrawn.status(), RunStatus::Cancelled);
        assert_eq!(withdrawn.started_at, None);
        assert_eq!(ids(&ws), vec![q2.id]);
        assert_eq!(ws.next_queued_run(goal.id).unwrap().unwrap().id, q2.id);
        assert_eq!(
            ws.get_goal(goal.id).unwrap().run,
            Some(live.id),
            "the current run is untouched"
        );
        assert_eq!(
            ws.get_goal(goal.id).unwrap().runs,
            vec![live.id, q1.id, q2.id]
        );
        assert_eq!(ws.dump_goal_rows().unwrap()[0].status, "running");
        assert!(matches!(
            ws.start_queued_run(q1.id),
            Err(StoreError::RunNotQueued { .. })
        ));
        let listed: Vec<RunId> = ws
            .list_runs(goal.id)
            .unwrap()
            .iter()
            .map(|r| r.id)
            .collect();
        assert_eq!(
            listed,
            vec![live.id, q1.id, q2.id],
            "history keeps the withdrawn run"
        );
    }

    /// A workflow's runs of the workspace are its own history: several go at
    /// once, none queues, and a goal's run of the same workflow is never
    /// among them.
    #[test]
    fn a_workflows_runs_of_the_workspace_are_listed_live_first_by_themselves() {
        let (_dir, ws) = ws();
        staffed(&ws);
        let wf = ws
            .create_workflow(approval_workflow(), bisa_core::WorkflowOrigin::Workspace)
            .unwrap();
        let other = ws
            .create_workflow(
                notify_workflow("Other"),
                bisa_core::WorkflowOrigin::Workspace,
            )
            .unwrap();
        let goal = ws.create_goal(NewGoal::captured("a goal's run")).unwrap();
        ws.create_run(
            on_goal(goal.id),
            wf.id,
            BTreeMap::new(),
            bisa_core::RunEntry::by_hand(),
            None,
        )
        .unwrap();
        let workspace = || RunScope::Workspace {
            budget: Default::default(),
        };
        let (first, effects) = ws
            .create_run(
                workspace(),
                wf.id,
                BTreeMap::new(),
                bisa_core::RunEntry::by_hand(),
                None,
            )
            .unwrap();
        assert_eq!(effects, vec![RunEffect::StartAgent { step: sid("build") }]);
        let (second, effects) = ws
            .create_run(
                workspace(),
                wf.id,
                BTreeMap::new(),
                bisa_core::RunEntry::by_hand(),
                None,
            )
            .unwrap();
        assert_eq!(
            effects,
            vec![RunEffect::StartAgent { step: sid("build") }],
            "a run of the workspace never queues behind another"
        );
        assert_eq!(second.status(), RunStatus::Running);
        let (stopped, _) = ws
            .create_run(
                workspace(),
                wf.id,
                BTreeMap::new(),
                bisa_core::RunEntry::by_hand(),
                None,
            )
            .unwrap();
        ws.record_run_event(
            stopped.id,
            RunEvent::Cancel {
                cause: CancelCause::Stopped { rationale: None },
            },
        )
        .unwrap();
        // Another workflow's run is another workflow's.
        let (elsewhere, _) = ws
            .create_run(
                workspace(),
                other.id,
                BTreeMap::new(),
                bisa_core::RunEntry::by_hand(),
                None,
            )
            .unwrap();

        let ids = |runs: Vec<WorkflowRun>| runs.iter().map(|r| r.id).collect::<Vec<_>>();
        assert_eq!(
            ids(ws.live_workspace_runs(Some(wf.id)).unwrap()),
            vec![first.id, second.id],
            "a stopped run is not going, and a goal's run is the goal's"
        );
        assert_eq!(
            ids(ws.list_workflow_runs(wf.id).unwrap()),
            vec![first.id, second.id, stopped.id],
            "the history keeps the stopped run, oldest first"
        );
        let live_everywhere = ids(ws.live_workspace_runs(None).unwrap());
        assert!(
            live_everywhere.contains(&first.id) && live_everywhere.contains(&second.id),
            "{live_everywhere:?}"
        );
        assert!(
            !live_everywhere.contains(&stopped.id),
            "{live_everywhere:?}"
        );
        // The other workflow's run is going too — its post is the engine's
        // to make — and it is that workflow's alone.
        assert!(!ws.get_run(elsewhere.id).unwrap().is_finished());
        assert!(
            live_everywhere.contains(&elsewhere.id),
            "{live_everywhere:?}"
        );
        assert_eq!(
            ids(ws.list_workflow_runs(other.id).unwrap()),
            vec![elsewhere.id]
        );
        assert_eq!(
            ids(ws.live_workspace_runs(Some(other.id)).unwrap()),
            vec![elsewhere.id]
        );
    }

    /// A run of the workspace is read from its own folder by its id — no
    /// index row is needed to find it — and is never started from a queue.
    #[test]
    fn a_workspace_run_is_found_by_its_folder_and_never_queued() {
        let (_dir, ws) = ws();
        staffed(&ws);
        let wf = ws
            .create_workflow(approval_workflow(), bisa_core::WorkflowOrigin::Workspace)
            .unwrap();
        let (run, _) = ws
            .create_run(
                RunScope::Workspace {
                    budget: Default::default(),
                },
                wf.id,
                BTreeMap::new(),
                bisa_core::RunEntry::by_hand(),
                None,
            )
            .unwrap();
        assert!(ws.paths().home(&run.home()).run_snapshot(run.id).is_file());
        assert_eq!(
            ws.idx().goal_of_run(&run.id.to_string()).unwrap(),
            Some(None),
            "its row names no goal"
        );
        ws.idx().clear().unwrap();
        assert_eq!(
            ws.get_run(run.id).unwrap().id,
            run.id,
            "found by its folder alone"
        );
        let err = ws.start_queued_run(run.id).unwrap_err();
        assert!(err.is_refusal(), "{err}");
        assert!(
            matches!(&err, StoreError::Invalid(text) if text.id == "error-store-invalid-workspace-run-never-queued"),
            "{err:?}"
        );
        assert!(err.to_string().contains(&run.id.to_string()), "{err}");
    }

    #[test]
    fn closing_a_goal_cancels_its_live_run_and_its_queue() {
        let (_dir, ws) = ws();
        staffed(&ws);
        let wf = ws
            .create_workflow(approval_workflow(), bisa_core::WorkflowOrigin::Workspace)
            .unwrap();
        let goal = ws.create_goal(NewGoal::captured("close me")).unwrap();
        let (run, _) = ws
            .create_run(
                on_goal(goal.id),
                wf.id,
                BTreeMap::new(),
                bisa_core::RunEntry::by_hand(),
                None,
            )
            .unwrap();
        let (queued, _) = ws
            .create_run(
                on_goal(goal.id),
                wf.id,
                BTreeMap::new(),
                bisa_core::RunEntry::by_hand(),
                None,
            )
            .unwrap();
        let reason = ClosureReason::Abandoned {
            rationale: Some("moved on".into()),
        };
        let (closed, cancelled) = ws.set_goal_closed(goal.id, reason.clone()).unwrap();
        assert!(closed.is_closed());
        let cause = CancelCause::Closed { reason };
        assert_eq!(
            cancelled
                .iter()
                .map(|(r, e)| (r.id, e.clone()))
                .collect::<Vec<_>>(),
            vec![
                (
                    queued.id,
                    vec![RunEffect::Cancelled {
                        cause: cause.clone()
                    }]
                ),
                (
                    run.id,
                    vec![
                        RunEffect::CancelWork {
                            steps: vec![sid("build")]
                        },
                        RunEffect::Cancelled {
                            cause: cause.clone()
                        }
                    ]
                ),
            ],
            "the queue is withdrawn first, then the live run is cancelled"
        );
        assert_eq!(ws.get_run(run.id).unwrap().status(), RunStatus::Cancelled);
        assert_eq!(ws.get_run(run.id).unwrap().cancelled, Some(cause.clone()));
        assert_eq!(
            ws.get_run(queued.id).unwrap().status(),
            RunStatus::Cancelled
        );
        assert_eq!(ws.get_run(queued.id).unwrap().cancelled, Some(cause));
        assert!(ws.queued_runs(goal.id).unwrap().is_empty());
        assert_eq!(ws.dump_goal_rows().unwrap()[0].status, "closed");
        assert_eq!(
            ws.dump_goal_rows().unwrap()[0].closure.as_deref(),
            Some("abandoned")
        );
        assert!(ws
            .set_goal_closed(goal.id, ClosureReason::Abandoned { rationale: None })
            .is_err());
        assert!(matches!(
            ws.create_run(
                on_goal(goal.id),
                wf.id,
                BTreeMap::new(),
                bisa_core::RunEntry::by_hand(),
                None
            ),
            Err(StoreError::Invalid(_))
        ));
        assert!(matches!(
            ws.start_queued_run(queued.id),
            Err(StoreError::RunNotQueued { .. })
        ));
        assert_eq!(
            run_facts(&ws, goal.id),
            vec![
                format!("{}:started", run.id),
                format!("{}:queued", queued.id),
                format!("{}:cancelled", queued.id),
                format!("{}:cancelled", run.id),
            ]
        );
        // An edit cannot close, reopen or re-run a goal.
        let mut sneaky = ws.get_goal(goal.id).unwrap();
        sneaky.closed = None;
        assert!(ws.update_goal(sneaky).is_err());
    }

    #[test]
    fn inputs_are_typed_defaulted_and_required() {
        let (_dir, ws) = ws();
        let mut new = notify_workflow("Inputs");
        new.inputs = vec![
            InputDef {
                name: bisa_core::InputName::new("env").unwrap(),
                label: "Env".into(),
                kind: InputKind::Choice {
                    options: vec!["dev".into(), "prod".into()],
                },
                default: Some(json!("dev")),
                required: false,
            },
            InputDef {
                name: bisa_core::InputName::new("count").unwrap(),
                label: "Count".into(),
                kind: InputKind::Number,
                default: None,
                required: true,
            },
        ];
        new.steps[0].kind = StepKind::Notify {
            scope: None,
            template: "{inputs.env} x{inputs.count}".into(),
            mentions: vec![],
            author: None,
        };
        let wf = ws
            .create_workflow(new, bisa_core::WorkflowOrigin::Workspace)
            .unwrap();
        let goal = ws.create_goal(NewGoal::captured("typed")).unwrap();
        assert!(
            ws.create_run(
                on_goal(goal.id),
                wf.id,
                BTreeMap::new(),
                bisa_core::RunEntry::by_hand(),
                None
            )
            .is_err(),
            "count is required"
        );
        assert!(ws
            .create_run(
                on_goal(goal.id),
                wf.id,
                BTreeMap::from([("count".to_string(), json!("two"))]),
                bisa_core::RunEntry::by_hand(),
                None
            )
            .is_err());
        assert!(ws
            .create_run(
                on_goal(goal.id),
                wf.id,
                BTreeMap::from([
                    ("count".to_string(), json!(2)),
                    ("env".to_string(), json!("qa"))
                ]),
                bisa_core::RunEntry::by_hand(),
                None
            )
            .is_err());
        assert!(ws
            .create_run(
                on_goal(goal.id),
                wf.id,
                BTreeMap::from([
                    ("count".to_string(), json!(2)),
                    ("extra".to_string(), json!(1))
                ]),
                bisa_core::RunEntry::by_hand(),
                None
            )
            .is_err());
        let (run, _) = ws
            .create_run(
                on_goal(goal.id),
                wf.id,
                BTreeMap::from([("count".to_string(), json!(2))]),
                bisa_core::RunEntry::by_hand(),
                None,
            )
            .unwrap();
        assert_eq!(run.inputs["env"], json!("dev"), "the default was filled in");
    }

    #[test]
    fn an_amendment_is_validated_and_versioned() {
        let (_dir, ws) = ws();
        staffed(&ws);
        let wf = ws
            .create_workflow(approval_workflow(), bisa_core::WorkflowOrigin::Workspace)
            .unwrap();
        let goal = ws.create_goal(NewGoal::captured("amend")).unwrap();
        let (run, _) = ws
            .create_run(
                on_goal(goal.id),
                wf.id,
                BTreeMap::new(),
                bisa_core::RunEntry::by_hand(),
                None,
            )
            .unwrap();
        let mut bad = run.workflow.clone();
        bad.steps[1].then.push(bisa_core::Flow::to(sid("zzz")));
        assert!(matches!(
            ws.record_run_event(run.id, RunEvent::Amended { workflow: bad }),
            Err(StoreError::WorkflowInvalid(_))
        ));
        let mut good = run.workflow.clone();
        good.steps.insert(
            2,
            step(
                "confirm",
                StepKind::Human {
                    prompt: "Really?".into(),
                    options: vec![AskOption::new("yes", "Yes")],
                    multi: false,
                    assignee: None,
                },
                &["end"],
            ),
        );
        good.steps[1].then = vec![bisa_core::Flow::to(sid("confirm"))];
        good.revision = 7;
        let (amended, effects) = ws
            .record_run_event(run.id, RunEvent::Amended { workflow: good })
            .unwrap();
        assert!(effects.is_empty(), "nothing new is ready yet: {effects:?}");
        assert_eq!(amended.workflow.steps.len(), 4);
        assert_eq!(amended.revision, 2);
        let amends: Vec<u64> = ws
            .journal(&home_of(goal.id))
            .unwrap()
            .into_iter()
            .filter_map(|je| match je.payload {
                JournalPayload::Run {
                    event: RunFact::Amended { revision },
                    ..
                } => Some(revision),
                _ => None,
            })
            .collect();
        assert_eq!(amends, vec![7]);
        // A human step's answer is validated against its options.
        let _ = Answer::text("later");
    }

    #[test]
    fn armed_waits_are_listed() {
        let (_dir, ws) = ws();
        let mut new = notify_workflow("Waits");
        new.steps.insert(
            0,
            step(
                "hold",
                StepKind::Wait {
                    until: WaitFor::Signal {
                        filter: bisa_core::SignalFilter {
                            name: "deploy.finished".into(),
                            fields: BTreeMap::new(),
                        },
                    },
                },
                &["post"],
            ),
        );
        let wf = ws
            .create_workflow(new, bisa_core::WorkflowOrigin::Workspace)
            .unwrap();
        let goal = ws.create_goal(NewGoal::captured("wait")).unwrap();
        let (run, effects) = ws
            .create_run(
                on_goal(goal.id),
                wf.id,
                BTreeMap::new(),
                bisa_core::RunEntry::by_hand(),
                None,
            )
            .unwrap();
        assert!(matches!(effects[0], RunEffect::Arm { .. }));
        let armed = ws.list_armed_waits().unwrap();
        assert_eq!(armed.len(), 1);
        assert_eq!(armed[0].0, run.id);
        assert_eq!(armed[0].1, sid("hold"));
        ws.set_wait_due(run.id, &sid("hold"), Some(500)).unwrap();
        assert_eq!(ws.list_due_waits(600).unwrap(), vec![(run.id, sid("hold"))]);
        assert!(ws.list_due_waits(100).unwrap().is_empty());
        ws.record_run_event(
            run.id,
            RunEvent::Heard {
                step: sid("hold"),
                payload: json!({"env": "prod"}),
                chain: bisa_core::Chain::default(),
            },
        )
        .unwrap();
        assert!(ws.list_armed_waits().unwrap().is_empty());
    }

    /// Two agent steps running in parallel finish within the same instant, on
    /// two threads. Both records land: the second writer cannot overwrite the
    /// first's step with a copy that never saw it.
    #[test]
    fn concurrent_events_on_one_run_never_lose_an_update() {
        let (_dir, ws) = ws();
        staffed(&ws);
        let wf = ws
            .create_workflow(
                NewWorkflow {
                    name: "Fan out".into(),
                    description: String::new(),
                    inputs: vec![],
                    steps: vec![
                        agent_step("kick", "developer", &["a", "b"]),
                        agent_step("a", "developer", &["end"]),
                        agent_step("b", "developer", &["end"]),
                        step(
                            "end",
                            StepKind::End {
                                finish: bisa_core::Finish::Done,
                            },
                            &[],
                        ),
                    ],
                    tags: Tags::default(),
                    decision_making: false,
                },
                bisa_core::WorkflowOrigin::Workspace,
            )
            .unwrap();
        let goal = ws.create_goal(NewGoal::captured("race")).unwrap();
        let (run, _) = ws
            .create_run(
                on_goal(goal.id),
                wf.id,
                BTreeMap::new(),
                bisa_core::RunEntry::by_hand(),
                None,
            )
            .unwrap();
        ws.record_run_event(
            run.id,
            RunEvent::StepDone {
                step: sid("kick"),
                output: json!({}),
            },
        )
        .unwrap();
        let run_id = run.id;
        std::thread::scope(|scope| {
            for name in ["a", "b"] {
                let ws = &ws;
                scope.spawn(move || {
                    ws.record_run_event(
                        run_id,
                        RunEvent::StepDone {
                            step: sid(name),
                            output: json!({ "who": name }),
                        },
                    )
                    .unwrap();
                });
            }
        });
        let after = ws.get_run(run_id).unwrap();
        assert!(
            matches!(after.steps[&sid("a")].state, StepState::Done { .. }),
            "{:?}",
            after.steps
        );
        assert!(
            matches!(after.steps[&sid("b")].state, StepState::Done { .. }),
            "{:?}",
            after.steps
        );
        assert_eq!(after.steps[&sid("a")].output, Some(json!({ "who": "a" })));
        assert_eq!(after.steps[&sid("b")].output, Some(json!({ "who": "b" })));
        assert!(after.is_finished(), "both joined into the end");
        // Four events on the run, one revision each, none lost.
        assert_eq!(after.revision, 4);
    }

    #[test]
    fn create_run_refuses_another_goals_design() {
        let (_dir, ws) = ws();
        let mine = ws.create_goal(NewGoal::captured("mine")).unwrap();
        let theirs = ws.create_goal(NewGoal::captured("theirs")).unwrap();
        let design = ws
            .create_workflow(
                notify_workflow("Design"),
                bisa_core::WorkflowOrigin::Goal { goal: mine.id },
            )
            .unwrap();
        let err = ws
            .create_run(
                on_goal(theirs.id),
                design.id,
                BTreeMap::new(),
                bisa_core::RunEntry::by_hand(),
                None,
            )
            .unwrap_err();
        assert!(
            err.to_string().contains("promote it to the library"),
            "{err}"
        );
        assert!(err.is_refusal());
        assert!(
            ws.get_current_run(theirs.id).unwrap().is_none(),
            "nothing was written"
        );
        // Its own goal runs it fine.
        ws.create_run(
            on_goal(mine.id),
            design.id,
            BTreeMap::new(),
            bisa_core::RunEntry::by_hand(),
            None,
        )
        .unwrap();
    }

    #[test]
    fn amend_rechecks_inputs_through_the_store() {
        let (_dir, ws) = ws();
        staffed(&ws);
        let wf = ws
            .create_workflow(approval_workflow(), bisa_core::WorkflowOrigin::Workspace)
            .unwrap();
        let goal = ws.create_goal(NewGoal::captured("amend inputs")).unwrap();
        let (run, _) = ws
            .create_run(
                on_goal(goal.id),
                wf.id,
                BTreeMap::new(),
                bisa_core::RunEntry::by_hand(),
                None,
            )
            .unwrap();

        // A new required input with no default: the run cannot bind it.
        let mut next = run.workflow.clone();
        next.revision += 1;
        next.inputs.push(InputDef {
            name: "who".parse().unwrap(),
            label: "Who".into(),
            kind: bisa_core::InputKind::Text,
            default: None,
            required: true,
        });
        next.steps[1].kind = StepKind::Approval {
            prompt: "Ship for {inputs.who}?".into(),
        };
        let err = ws
            .record_run_event(
                run.id,
                RunEvent::Amended {
                    workflow: next.clone(),
                },
            )
            .unwrap_err();
        assert!(
            matches!(
                err,
                StoreError::Run(bisa_core::RunError::AmendNeedsInput(
                    bisa_core::InputError::Missing { .. }
                ))
            ),
            "{err:?}"
        );
        assert_eq!(
            ws.get_run(run.id).unwrap().revision,
            run.revision,
            "nothing was written"
        );

        // With a default, the amendment lands and the run holds the value.
        next.inputs[0].default = Some(json!("us"));
        next.inputs[0].required = false;
        let (amended, _) = ws
            .record_run_event(run.id, RunEvent::Amended { workflow: next })
            .unwrap();
        assert_eq!(amended.inputs.get("who"), Some(&json!("us")));

        // Another workflow's id under the run: refused before anything moves.
        let mut other = amended.workflow.clone();
        other.id = WorkflowId::from_ulid(ulid::Ulid::from_parts(5, 5));
        other.revision += 1;
        let err = ws
            .record_run_event(run.id, RunEvent::Amended { workflow: other })
            .unwrap_err();
        assert!(
            matches!(
                err,
                StoreError::Run(bisa_core::RunError::AmendChangesWorkflow { .. })
            ),
            "{err:?}"
        );
    }

    #[test]
    fn runs_and_steps_survive_a_rebuild() {
        let (_dir, ws) = ws();
        staffed(&ws);
        let wf = ws
            .create_workflow(approval_workflow(), bisa_core::WorkflowOrigin::Workspace)
            .unwrap();
        let goal = ws.create_goal(NewGoal::captured("rebuild")).unwrap();
        let (run, _) = ws
            .create_run(
                on_goal(goal.id),
                wf.id,
                BTreeMap::new(),
                bisa_core::RunEntry::by_hand(),
                None,
            )
            .unwrap();
        ws.record_run_event(
            run.id,
            RunEvent::StepDone {
                step: sid("build"),
                output: json!({}),
            },
        )
        .unwrap();
        let (queued, _) = ws
            .create_run(
                on_goal(goal.id),
                wf.id,
                BTreeMap::new(),
                bisa_core::RunEntry::by_hand(),
                None,
            )
            .unwrap();
        let goals_before = ws.dump_goal_rows().unwrap();
        let run_before = ws.idx().get_run(&run.id.to_string()).unwrap();
        let queued_before = ws.idx().get_run(&queued.id.to_string()).unwrap();
        assert_eq!(queued_before.as_ref().unwrap().status, "queued");
        assert_eq!(queued_before.as_ref().unwrap().started_at, None);
        ws.rebuild_index().unwrap();
        assert_eq!(ws.dump_goal_rows().unwrap(), goals_before);
        assert_eq!(ws.idx().get_run(&run.id.to_string()).unwrap(), run_before);
        assert_eq!(
            ws.idx().get_run(&queued.id.to_string()).unwrap(),
            queued_before,
            "a queued row rebuilds from its snapshot"
        );
        assert_eq!(ws.get_current_run(goal.id).unwrap().unwrap().id, run.id);
        assert_eq!(
            ws.idx()
                .runs_for_goal(&goal.id.to_string())
                .unwrap()
                .iter()
                .map(|r| r.id.clone())
                .collect::<Vec<_>>(),
            vec![run.id.to_string(), queued.id.to_string()]
        );
        assert_eq!(ws.next_queued_run(goal.id).unwrap().unwrap().id, queued.id);
    }

    /// A goal cannot point at another goal's design; the refusal says what to
    /// do instead.
    #[test]
    fn a_goal_cannot_use_another_goals_design() {
        let (_dir, ws) = ws();
        staffed(&ws);
        let a = ws.create_goal(NewGoal::captured("mine")).unwrap();
        let b = ws.create_goal(NewGoal::captured("theirs")).unwrap();
        let design = ws
            .create_workflow(
                approval_workflow(),
                bisa_core::WorkflowOrigin::Goal { goal: a.id },
            )
            .unwrap();
        ws.set_goal_workflow(a.id, Some(design.id)).unwrap();
        let err = ws.set_goal_workflow(b.id, Some(design.id)).unwrap_err();
        assert!(
            err.to_string().contains("promote it to the library"),
            "{err}"
        );
    }
}
