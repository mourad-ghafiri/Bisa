//! Workflow definitions (kind 33412).
//!
//! Truth is the signed snapshot — a definition reads the same on every node,
//! so it travels and needs no JSON twin. Where it is filed follows its origin
//!: a library workflow (`workspace`, `catalog`) in
//! `workflows/state/33412-<id>.json`; a goal's **design** (`goal`) in
//! `goals/<goal>/state/33412-<id>.json`, beside the goal's runs, so it syncs
//! with the goal and goes with its folder. [`workflow_namespace`] is the one
//! rule, and the `workflows` index row's `goal_id` is how a reader finds the
//! file. The catalog's unique partial index on `catalog_slug` is what makes "a
//! template is installed at most once" the schema's promise as well as the
//! installer's.
//!
//! Every write re-validates against what the workspace knows — who exists,
//! which workflows and projects exist, which harnesses and models the engine
//! described ([`crate::workspace::KnownRuntime`]), which workflows spawn
//! which, and whether a cron expression or a schema parses
//! ([`crate::syntax::StoreSyntaxChecks`]) — and a problem list is what comes
//! back, never the first problem: the designer shows them all at once.
//!
//! **One revision rule.** An edit names the revision it was made from and is
//! written through the snapshot store's compare-and-swap; a stale edit is a
//! [`StoreError::RevisionConflict`] naming both revisions, never a silent
//! overwrite. A *save* refuses problems (`create_workflow`, `update_workflow`);
//! a *draft* keeps them and returns them (`create_workflow_draft`,
//! `save_workflow_draft`) — the designer's autosave, where a half-connected
//! graph is the normal state between two keystrokes. `Adopt` and `Start`
//! validate again before anything runs.

use crate::error::StoreError;
use crate::index::WorkflowRow;
use crate::paths::Paths;
use crate::syntax::StoreSyntaxChecks;
use crate::usage::UsageKind;
use crate::workspace::{mint_ulid, now_secs, EventAudience, StoreEvent, Workspace};
use bisa_core::kind::KIND_WORKFLOW;
use bisa_core::tags::TagEntity;
use bisa_core::{
    Assignee, GoalId, InputDef, ListenerHost, PrincipalId, Problem, ProjectId, StartOn, Step,
    StepId, StepKind, Tags, ValidationCtx, Workflow, WorkflowId, WorkflowOrigin,
};
use std::collections::BTreeSet;

/// What a caller supplies to create or edit a workflow; the store adds the
/// id, the author, the origin, the revision and the clock. Also the wire
/// shape a proposal arrives in, from the Workflow Agent's tool or a designer's
/// save. A key it does not know is refused: a misspelled `descripton` is an
/// error, not a description that silently stays empty.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewWorkflow {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub inputs: Vec<InputDef>,
    pub steps: Vec<Step>,
    #[serde(default)]
    pub tags: Tags,
    /// Whether the Decision-Making Agent stands in at the decision points a
    /// run of this workflow reaches ([`Workflow::decision_making`]).
    #[serde(default)]
    pub decision_making: bool,
}

impl From<&Workflow> for NewWorkflow {
    /// The editable content of a stored workflow — what an edit, a promotion
    /// or an amendment starts from.
    fn from(wf: &Workflow) -> Self {
        NewWorkflow {
            name: wf.name.clone(),
            description: wf.description.clone(),
            inputs: wf.inputs.clone(),
            steps: wf.steps.clone(),
            tags: wf.tags.clone(),
            decision_making: wf.decision_making,
        }
    }
}

/// The workspace's directory as validation reads it: every enabled agent and
/// team, every member, every workflow, every project, the runtime the engine
/// described, every workflow's spawn targets, the workflows only events
/// start, and the store's syntax checks. Owned, because the borrows a
/// [`ValidationCtx`] takes have to outlive the store calls that build them.
pub struct OwnedValidationCtx {
    assignees: Vec<Assignee>,
    workflows: Vec<WorkflowId>,
    projects: Vec<ProjectId>,
    harnesses: Vec<String>,
    models: Vec<(String, Vec<String>)>,
    effort_harnesses: Vec<String>,
    spawns: Vec<(WorkflowId, Vec<WorkflowId>)>,
    connectors: Vec<bisa_core::Connector>,
    accounts: Vec<bisa_core::ConnectorAccount>,
    topics: &'static [&'static str],
    event_only: Vec<WorkflowId>,
    asks: Vec<(WorkflowId, Vec<bisa_core::InputDef>)>,
    checks: StoreSyntaxChecks,
}

impl OwnedValidationCtx {
    pub fn as_ctx(&self) -> ValidationCtx<'_> {
        ValidationCtx {
            assignees: &self.assignees,
            workflows: &self.workflows,
            projects: &self.projects,
            harnesses: &self.harnesses,
            models: &self.models,
            effort_harnesses: &self.effort_harnesses,
            spawns: &self.spawns,
            connectors: &self.connectors,
            accounts: &self.accounts,
            topics: self.topics,
            event_only: &self.event_only,
            asks: &self.asks,
            checks: &self.checks,
        }
    }
}

/// Which workflows a listing wants: the library (the workspace's own designs
/// and the installed templates), one goal's designs, or everything.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkflowScope {
    Library,
    Goal(GoalId),
    All,
}

impl WorkflowScope {
    fn admits(self, wf: &Workflow) -> bool {
        match self {
            WorkflowScope::All => true,
            WorkflowScope::Library => wf.origin.is_library(),
            WorkflowScope::Goal(goal) => wf.origin.goal() == Some(goal),
        }
    }
}

impl Workspace {
    /// Create a workflow under the origin the caller — always the engine or
    /// the catalog installer — derived. Refused with every problem when the
    /// definition cannot start as written. A caller never sends an origin on
    /// the wire (I28a); this argument is the engine naming what it knows.
    pub fn create_workflow(
        &self,
        new: NewWorkflow,
        origin: WorkflowOrigin,
    ) -> Result<Workflow, StoreError> {
        self.create_workflow_as(new, origin, self.owner_principal())
    }

    /// [`Workspace::create_workflow`] with the author named: an agent's
    /// proposal is the agent's in the record, not the owner's.
    pub fn create_workflow_as(
        &self,
        new: NewWorkflow,
        origin: WorkflowOrigin,
        author: PrincipalId,
    ) -> Result<Workflow, StoreError> {
        let wf = self.mint_workflow(new, origin, author)?;
        self.refuse_problems(&wf)?;
        self.write_workflow(&wf)?;
        Ok(wf)
    }

    /// Create a workflow **keeping** its problems, which come back with it —
    /// the designer's first save of a new canvas, or a design a person is
    /// still drawing. The catalog installer, a proposal and a promotion use
    /// [`Workspace::create_workflow`], which refuses. `Adopt` and `Start`
    /// validate again before anything runs.
    pub fn create_workflow_draft(
        &self,
        new: NewWorkflow,
        origin: WorkflowOrigin,
    ) -> Result<(Workflow, Vec<Problem>), StoreError> {
        let wf = self.mint_workflow(new, origin, self.owner_principal())?;
        let problems = self.validate_workflow(&wf)?;
        self.write_workflow(&wf)?;
        Ok((wf, problems))
    }

    /// A fresh definition at revision 1 under `origin`, not yet written. A
    /// catalog origin whose slug is already installed is refused here, so
    /// neither creating path can install a template twice.
    fn mint_workflow(
        &self,
        new: NewWorkflow,
        origin: WorkflowOrigin,
        author: PrincipalId,
    ) -> Result<Workflow, StoreError> {
        if let Some(slug) = origin.catalog_slug() {
            let installed = self.idx().workflow_id_for_slug(slug)?;
            if let Some(existing) = installed {
                return Err(StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-catalog-workflow-already-installed-as",
                    slug = format!("{slug:?}"),
                    existing = existing.to_string()
                )));
            }
        }
        Ok(Workflow {
            id: WorkflowId::from_ulid(mint_ulid()),
            name: new.name,
            description: new.description,
            inputs: new.inputs,
            steps: new.steps,
            origin,
            author,
            tags: new.tags,
            revision: 1,
            archived: None,
            decision_making: new.decision_making,
            created_at: now_secs(),
        })
    }

    /// Copy a goal's design into the library: a new id, revision 1, origin
    /// `Workspace`, same author and content. The original stays the goal's —
    /// a run may hold it, and history is not rewritten. Refused
    /// ([`StoreError::AlreadyLibrary`]) for anything already in the library.
    pub fn promote_workflow(&self, id: WorkflowId) -> Result<Workflow, StoreError> {
        let design = self.get_workflow(id)?;
        if design.origin.is_library() {
            return Err(StoreError::AlreadyLibrary(design.name));
        }
        self.create_workflow(NewWorkflow::from(&design), WorkflowOrigin::Workspace)
    }

    /// Persist an edited definition as the next revision of `id`. The
    /// origin, author and birth are the stored copy's; `expected_revision` is
    /// the revision the caller edited, and a stored copy that has moved past
    /// it is a [`StoreError::RevisionConflict`] — nothing is written. A save
    /// that cannot start is refused with its problems; a designer that wants
    /// to keep an unfinished draft uses [`Workspace::save_workflow_draft`].
    pub fn update_workflow(
        &self,
        id: WorkflowId,
        body: NewWorkflow,
        expected_revision: u64,
    ) -> Result<Workflow, StoreError> {
        let wf = self.next_revision(id, body, expected_revision)?;
        self.refuse_problems(&wf)?;
        self.write_workflow_expecting(&wf, expected_revision)?;
        Ok(wf)
    }

    /// Persist an edited definition **without** refusing its problems — the
    /// designer's autosave, where a half-connected graph is the normal state
    /// between two keystrokes. The revision rule is the same as
    /// [`Workspace::update_workflow`]'s. What cannot be saved is a definition
    /// that does not even parse as one, which the type system already
    /// refuses. `Adopt` and `Start` validate again before anything runs.
    pub fn save_workflow_draft(
        &self,
        id: WorkflowId,
        body: NewWorkflow,
        expected_revision: u64,
    ) -> Result<(Workflow, Vec<Problem>), StoreError> {
        let wf = self.next_revision(id, body, expected_revision)?;
        let problems = self.validate_workflow(&wf)?;
        self.write_workflow_expecting(&wf, expected_revision)?;
        Ok((wf, problems))
    }

    /// The stored workflow with `body`'s content at `expected_revision + 1`.
    /// The conflict check here is the early one, so a stale editor is told
    /// before validation runs; the snapshot store checks again at the write.
    /// A draft and a save alike may not take away a public hook start a
    /// listening host still answers on.
    fn next_revision(
        &self,
        id: WorkflowId,
        body: NewWorkflow,
        expected_revision: u64,
    ) -> Result<Workflow, StoreError> {
        let stored = self.get_workflow(id)?;
        if stored.revision != expected_revision {
            return Err(StoreError::RevisionConflict {
                kind: "workflow",
                id: id.to_string(),
                expected: expected_revision,
                actual: stored.revision,
            });
        }
        let next = Workflow {
            name: body.name,
            description: body.description,
            inputs: body.inputs,
            steps: body.steps,
            tags: body.tags,
            decision_making: body.decision_making,
            revision: expected_revision + 1,
            ..stored.clone()
        };
        self.refuse_dropping_a_listening_hook(&stored, &next)?;
        Ok(next)
    }

    /// A public hook start is a door someone outside was given, with its
    /// secret: an edit that removes it, renames it or makes it local while a
    /// host listens on it would break that caller without a word. Refused —
    /// turn the host off first. A local hook, or any other start, may change
    /// freely: the registry follows the definition.
    fn refuse_dropping_a_listening_hook(
        &self,
        stored: &Workflow,
        next: &Workflow,
    ) -> Result<(), StoreError> {
        let kept = public_hook_steps(next);
        let dropped: Vec<StepId> = public_hook_steps(stored)
            .into_iter()
            .filter(|step| !kept.contains(step))
            .collect();
        let Some(step) = dropped.first() else {
            return Ok(());
        };
        if self.listening_hosts_of(stored)?.is_empty() {
            return Ok(());
        }
        Err(StoreError::StillUsed(bisa_core::text!(
            "error-store-still-used-listening-public-hook",
            step = step.to_string(),
            workflow = stored.name.clone()
        )))
    }

    /// The hosts listening with `wf` right now: the workspace, when a library
    /// workflow is On, and every listening goal whose workflow it is.
    pub(crate) fn listening_hosts_of(
        &self,
        wf: &Workflow,
    ) -> Result<Vec<ListenerHost>, StoreError> {
        let mut out = Vec::new();
        if wf.origin.is_library() {
            let host = ListenerHost::Workspace { workflow: wf.id };
            if self.listening(&host)?.is_some() {
                out.push(host);
            }
        }
        let goals = self.idx().listening_goal_ids()?;
        for id in goals {
            let Ok(goal) = id.parse::<GoalId>() else {
                continue;
            };
            match self.get_goal(goal) {
                Ok(g) if g.listening.is_some() && g.workflow == Some(wf.id) => {
                    out.push(ListenerHost::Goal { goal });
                }
                Ok(_) | Err(StoreError::GoalNotFound(_)) => {}
                Err(e) => return Err(e),
            }
        }
        Ok(out)
    }

    pub fn get_workflow(&self, id: WorkflowId) -> Result<Workflow, StoreError> {
        let d = id.to_string();
        let ns = self.workflow_ns_of(id)?;
        match self.snapshots.get::<Workflow>(&ns, KIND_WORKFLOW, &d)? {
            Some((wf, _)) => Ok(wf),
            None => Err(StoreError::WorkflowNotFound(d)),
        }
    }

    /// The namespace holding `id`: the index row's `goal_id` says whether it
    /// is a goal's design or a library workflow. With no row yet — a snapshot
    /// applied but not indexed — the library is the one place to probe.
    fn workflow_ns_of(&self, id: WorkflowId) -> Result<String, StoreError> {
        let d = id.to_string();
        let owner = self.idx().get_workflow(&d)?.and_then(|row| row.goal_id);
        Ok(match owner {
            Some(goal) => Paths::ns_goal(goal.parse::<GoalId>().map_err(|e| {
                StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-bad-goal-id-workflows-index",
                    e = e.to_string()
                ))
            })?),
            None => Paths::NS_WORKFLOWS.to_string(),
        })
    }

    /// Every workflow, oldest first — the goals' designs included.
    pub fn list_workflows(&self) -> Result<Vec<Workflow>, StoreError> {
        let ids = self.idx().list_workflow_ids()?;
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            let id = id.parse::<WorkflowId>().map_err(|e| {
                StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-bad-workflow-id-index",
                    e = e.to_string()
                ))
            })?;
            match self.get_workflow(id) {
                Ok(wf) => out.push(wf),
                // Gone between the listing and the read — a held amendment
                // dropped as its run finished: not in the list, not an error.
                Err(StoreError::WorkflowNotFound(_)) => continue,
                Err(e) => return Err(e),
            }
        }
        Ok(out)
    }

    /// The workflows one scope shows: the library hides every goal's designs,
    /// and a goal's scope is its designs alone. Never an archived one.
    pub fn list_workflows_in(&self, scope: WorkflowScope) -> Result<Vec<Workflow>, StoreError> {
        Ok(self
            .list_workflows()?
            .into_iter()
            .filter(|wf| scope.admits(wf))
            .collect())
    }

    /// The workflows put away in one scope, newest first — what a list shows only when asked.
    pub fn list_archived_workflows_in(
        &self,
        scope: WorkflowScope,
    ) -> Result<Vec<Workflow>, StoreError> {
        let ids = self.idx().list_archived_workflow_ids()?;
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            let id = id.parse::<WorkflowId>().map_err(|e| {
                StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-bad-workflow-id-index",
                    e = e.to_string()
                ))
            })?;
            let wf = self.get_workflow(id)?;
            if scope.admits(&wf) {
                out.push(wf);
            }
        }
        Ok(out)
    }

    /// Put a workflow away, or take it back out: out of the library and the
    /// pickers, refused for a goal or a run while it is; the runs that hold a
    /// copy are untouched. Putting it away is refused while one of its runs
    /// of the workspace goes. A new revision, like any write.
    pub fn set_workflow_archived(
        &self,
        id: WorkflowId,
        archived: bool,
    ) -> Result<Workflow, StoreError> {
        let mut wf = self.get_workflow(id)?;
        if wf.is_archived() == archived {
            return Ok(wf);
        }
        // A run of the workspace runs its own copy, but it runs *as* the
        // workflow's: put away under it, it would go on out of sight. The
        // engine's retirement ends them first (I50).
        if archived {
            if let Some(live) = self.live_workspace_runs(Some(id))?.first() {
                return Err(StoreError::StillUsed(bisa_core::text!(
                    "error-store-still-used-workflow-has-run-going-archive",
                    a0 = (wf.name).to_string(),
                    run = live.id.to_string()
                )));
            }
        }
        wf.archived = archived.then(|| bisa_core::Archived::at(now_secs()));
        wf.revision += 1;
        self.write_workflow(&wf)?;
        self.index_workflow(&wf)?;
        Ok(wf)
    }

    /// Remove one goal's designs on the way out of the goal itself. The
    /// goal's own pointer does not hold a design in place, but anything else
    /// that names it — another workflow's spawn step, a start that hears its
    /// runs — does, and the refusal names it (the same rule `delete_workflow`
    /// applies).
    pub(crate) fn release_goal_designs(&self, goal: GoalId) -> Result<(), StoreError> {
        // Read first, then lock once: `goal_designs` reads through the
        // workspace, which locks the index itself.
        let designs = self.goal_designs(goal)?;
        let idx = self.idx();
        idx.in_transaction(|| {
            for id in &designs {
                // The files go with `goals/<id>/` when the caller removes the
                // folder; what has to go here is the rows — all of them, or none.
                let d = id.to_string();
                idx.delete_workflow(&d)?;
                idx.clear_tags(TagEntity::Workflow, &d)?;
            }
            Ok(())
        })
    }

    /// The goal's own designs, refused by name when one is still used by
    /// something other than the goal — the read-only half of a deletion,
    /// so a retirement can refuse before it stops anything.
    fn goal_designs(&self, goal: GoalId) -> Result<Vec<WorkflowId>, StoreError> {
        let mut out = Vec::new();
        let designs = self.idx().workflows_of_goal(&goal.to_string())?;
        for id in designs {
            let id = id.parse::<WorkflowId>().map_err(|e| {
                StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-bad-workflow-id-index",
                    e = e.to_string()
                ))
            })?;
            let usage = self.usage_of(UsageKind::Workflow, &id.to_string())?;
            let holders: Vec<String> = usage
                .as_slice()
                .iter()
                .filter(|r| {
                    !(r.kind == crate::usage::ReferenceKind::Goal && r.id == goal.to_string())
                })
                .map(|r| format!("{} {}", r.kind, r.id))
                .collect();
            if !holders.is_empty() {
                return Err(StoreError::Invalid(bisa_core::text!(
                    "error-store-invalid-goal-cannot-be-deleted-design-still-used",
                    goal = goal.to_string(),
                    a0 = (id).to_string(),
                    a1 = (holders.join(", ")).to_string()
                )));
            }
            out.push(id);
        }
        Ok(out)
    }

    /// Whether the goal may be deleted right now: every design of its own
    /// is used by nothing else. The refusal names the design and its holders.
    pub fn goal_deletable(&self, goal: GoalId) -> Result<(), StoreError> {
        self.get_goal(goal)?;
        self.goal_designs(goal).map(|_| ())
    }

    /// The workflow installed from a catalog slug, if any.
    pub fn workflow_for_slug(&self, slug: &str) -> Result<Option<Workflow>, StoreError> {
        // Bound first: a `match` on the locked read would hold the guard
        // through the arm that reads the workflow, which locks again.
        let found = self.idx().workflow_id_for_slug(slug)?;
        match found {
            Some(id) => {
                let id = id.parse::<WorkflowId>().map_err(|e| {
                    StoreError::Invalid(bisa_core::text!(
                        "error-store-invalid-bad-workflow-id-index",
                        e = e.to_string()
                    ))
                })?;
                Ok(Some(self.get_workflow(id)?))
            }
            None => Ok(None),
        }
    }

    /// Delete a definition. Refused by name while a goal, another workflow's
    /// `spawn` step or another workflow's `run` start points at it, and while
    /// one of its runs of the workspace still goes. A goal's run holds its
    /// own frozen copy, so a finished goal's history is untouched; the
    /// workflow's own runs of the workspace are its history and are deleted
    /// with it, and so is what it listened with — its record, its listeners'
    /// memories, its hook secrets, its queued signals.
    pub fn delete_workflow(&self, id: WorkflowId) -> Result<(), StoreError> {
        let wf = self.get_workflow(id)?;
        let d = id.to_string();
        self.refuse_if_used(UsageKind::Workflow, &d)?;
        // Its runs of the workspace are its history and go with it — refused
        // while one still goes; the engine retires them first.
        self.delete_workflow_runs(&wf)?;
        if wf.origin.is_library() {
            self.forget_host(&ListenerHost::Workspace { workflow: id }, &hook_steps(&wf))?;
        }
        self.snapshots
            .delete_snapshot(&workflow_namespace(&wf.origin), KIND_WORKFLOW, &d)?;
        // Its notes and drawings live in their repositories: they leave with
        // the definition.
        let scope = bisa_core::OwnerScope::Workflow { id };
        let scoped =
            |base: std::path::PathBuf| crate::paths::Paths::scoped_dir(&base, "workflow", Some(&d));
        self.remove_notes_of(scope.clone(), &scoped(self.paths.notes_dir()))?;
        self.remove_drawings_of(scope, &scoped(self.paths.drawings_dir()))?;
        // So do the conversations about it.
        self.remove_conversations_of("workflow", &d)?;
        let idx = self.idx();
        idx.delete_workflow(&d)?;
        idx.clear_tags(TagEntity::Workflow, &d)
    }

    /// Every problem with a definition, against what this workspace knows.
    /// Writes nothing.
    pub fn validate_workflow(&self, wf: &Workflow) -> Result<Vec<Problem>, StoreError> {
        let ctx = self.workflow_validation_ctx()?;
        Ok(wf.validate(&ctx.as_ctx()))
    }

    /// The directory validation resolves against: enabled agents and teams as
    /// assignees, every member as a human, every workflow (and which of them
    /// only events start), every project, the topics the engine emits.
    pub(crate) fn workflow_validation_ctx(&self) -> Result<OwnedValidationCtx, StoreError> {
        let mut assignees: Vec<Assignee> = Vec::new();
        for a in self.list_agents()? {
            if a.enabled {
                assignees.push(Assignee::Agent(a.id.to_string()));
            }
        }
        for t in self.list_teams()? {
            if t.enabled {
                assignees.push(Assignee::Team(t.id.to_string()));
            }
        }
        for m in self.members()? {
            assignees.push(Assignee::Human(m.pubkey));
        }
        let workflows = self
            .idx()
            .list_workflow_ids()?
            .into_iter()
            .filter_map(|id| id.parse().ok())
            .collect();
        let projects = self.list_projects()?.into_iter().map(|p| p.id).collect();
        let every = self.list_workflows()?;
        let spawns = every.iter().map(|wf| (wf.id, spawn_targets(wf))).collect();
        let event_only = every
            .iter()
            .filter(|wf| wf.is_event_only())
            .map(|wf| wf.id)
            .collect();
        let asks = every.iter().map(|wf| (wf.id, wf.inputs.clone())).collect();
        let runtime = self.known_runtime();
        let connectors = self.list_connectors()?;
        let accounts = self.list_all_connector_accounts()?;
        Ok(OwnedValidationCtx {
            assignees,
            workflows,
            projects,
            harnesses: runtime.harnesses,
            models: runtime.models,
            effort_harnesses: runtime.effort_harnesses,
            spawns,
            connectors,
            accounts,
            topics: runtime.topics,
            event_only,
            asks,
            checks: StoreSyntaxChecks,
        })
    }

    fn refuse_problems(&self, wf: &Workflow) -> Result<(), StoreError> {
        let problems = self.validate_workflow(wf)?;
        if problems.is_empty() {
            Ok(())
        } else {
            Err(StoreError::WorkflowInvalid(problems))
        }
    }

    /// The first write of a new id, at revision 1.
    fn write_workflow(&self, wf: &Workflow) -> Result<(), StoreError> {
        let d = wf.id.to_string();
        let event = self.snapshots.put(
            &workflow_namespace(&wf.origin),
            KIND_WORKFLOW,
            &d,
            wf,
            wf.revision,
            &self.owner,
            now_secs(),
            None,
            wf.tags.as_slice(),
        )?;
        self.announce_workflow_write(wf, event)
    }

    /// An edit: written only if the stored copy is still at `expected`.
    fn write_workflow_expecting(&self, wf: &Workflow, expected: u64) -> Result<(), StoreError> {
        let d = wf.id.to_string();
        let event = self.snapshots.put_expecting(
            &workflow_namespace(&wf.origin),
            KIND_WORKFLOW,
            &d,
            "workflow",
            wf,
            expected,
            &self.owner,
            now_secs(),
            None,
            wf.tags.as_slice(),
        )?;
        self.announce_workflow_write(wf, event)
    }

    fn announce_workflow_write(
        &self,
        wf: &Workflow,
        event: nostr::event::Event,
    ) -> Result<(), StoreError> {
        self.emit_store_event(StoreEvent::ConversationSnapshot {
            kind: KIND_WORKFLOW,
            d: wf.id.to_string(),
            event,
            audience: EventAudience::Workspace,
        });
        self.index_workflow(wf)
    }

    pub(crate) fn index_workflow(&self, wf: &Workflow) -> Result<(), StoreError> {
        let d = wf.id.to_string();
        let idx = self.idx();
        idx.upsert_workflow(&WorkflowRow {
            id: d.clone(),
            name: wf.name.clone(),
            origin: match wf.origin {
                WorkflowOrigin::Workspace => "workspace".into(),
                WorkflowOrigin::Catalog { .. } => "catalog".into(),
                WorkflowOrigin::Goal { .. } => "goal".into(),
            },
            catalog_slug: wf.origin.catalog_slug().map(str::to_string),
            goal_id: wf.origin.goal().map(|g| g.to_string()),
            author: wf.author.as_hex().to_string(),
            revision: wf.revision,
            step_count: wf.steps.len() as u64,
            archived_at: wf.archived.map(|a| a.at),
            created_at: wf.created_at,
        })?;
        idx.set_tags(TagEntity::Workflow, &d, wf.tags.as_slice())?;
        idx.index_text(
            &format!("{d}:workflow"),
            &d,
            &format!("{} {}", wf.name, wf.description),
        )
    }

    /// Rebuild support: every snapshot in `workflows/state/`.
    /// Every workflow on disk, from the library and from under every goal —
    /// a design is filed beside its goal's runs, so the walk goes there too.
    pub(crate) fn reindex_workflows(&self) -> Result<(), StoreError> {
        let mut namespaces = vec![Paths::NS_WORKFLOWS.to_string()];
        namespaces.extend(self.goal_ids_on_disk()?.into_iter().map(Paths::ns_goal));
        for ns in namespaces {
            for d in self.snapshots.list_ds(&ns, KIND_WORKFLOW)? {
                // A workflow this build cannot read is one error line and a
                // goal row with no workflow, never a failed rebuild.
                let read = self.snapshots.get::<Workflow>(&ns, KIND_WORKFLOW, &d);
                if let Some((wf, _)) = crate::workspace::tolerated("workflow", &d, read)?.flatten()
                {
                    if workflow_namespace(&wf.origin) != ns {
                        // A file under a goal whose origin says otherwise is a
                        // peer's mistake, not a reason to refuse the rebuild.
                        tracing::warn!(workflow = %d, namespace = %ns, "a workflow filed in the wrong namespace; skipped");
                        continue;
                    }
                    self.index_workflow(&wf)?;
                }
            }
        }
        Ok(())
    }
}

/// The start steps of `wf` that begin on a hook call, local or public — the
/// ones a hook secret may be kept for.
pub(crate) fn hook_steps(wf: &Workflow) -> Vec<StepId> {
    wf.steps
        .iter()
        .filter(|s| {
            matches!(
                s.kind,
                StepKind::Start {
                    on: StartOn::Hook { .. },
                    ..
                }
            )
        })
        .map(|s| s.id.clone())
        .collect()
}

/// The start steps of `wf` that answer a public hook call.
fn public_hook_steps(wf: &Workflow) -> BTreeSet<StepId> {
    wf.steps
        .iter()
        .filter(|s| {
            matches!(
                s.kind,
                StepKind::Start {
                    on: StartOn::Hook { public: true },
                    ..
                }
            )
        })
        .map(|s| s.id.clone())
        .collect()
}

/// The workflows a definition's `spawn` steps name, for the cycle check.
fn spawn_targets(wf: &Workflow) -> Vec<WorkflowId> {
    wf.steps
        .iter()
        .filter_map(|s| match &s.kind {
            StepKind::Spawn {
                workflow: Some(w), ..
            } => Some(*w),
            _ => None,
        })
        .collect()
}

/// Where a workflow's snapshot lives: a goal's design beside the
/// goal's runs, a library workflow in `workflows/`. The one rule; the index
/// row's `goal_id` is how a reader finds the file.
pub(crate) fn workflow_namespace(origin: &WorkflowOrigin) -> String {
    match origin {
        WorkflowOrigin::Goal { goal } => Paths::ns_goal(*goal),
        WorkflowOrigin::Workspace | WorkflowOrigin::Catalog { .. } => {
            Paths::NS_WORKFLOWS.to_string()
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::identity::MemoryKeyStore;
    use crate::workspace::NewGoal;
    use bisa_core::{
        Flow, Join, OnFail, ProblemKind, StepId, StepKind, ToolTier, ValueRef, DEFAULT_MAX_VISITS,
    };

    fn ws() -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        let w =
            Workspace::open_with_keystore(dir.path(), Box::new(MemoryKeyStore::default())).unwrap();
        (dir, w)
    }

    pub(crate) fn sid(s: &str) -> StepId {
        StepId::new(s).unwrap()
    }

    pub(crate) fn step(id: &str, kind: StepKind, then: &[&str]) -> Step {
        Step {
            id: sid(id),
            name: id.to_uppercase(),
            kind,
            then: then.iter().map(|t| Flow::to(sid(t))).collect(),
            boundaries: vec![],
            join: Join::All,
            on_fail: OnFail::Fail,
            retries: 0,
            max_visits: DEFAULT_MAX_VISITS,
            position: None,
        }
    }

    /// An agent step whose instructions read nothing a run of the workspace
    /// lacks, so a definition built of these runs in either scope.
    pub(crate) fn agent_step(id: &str, agent: &str, then: &[&str]) -> Step {
        step(
            id,
            StepKind::Agent {
                instructions: format!("do {id}"),
                assignee: Some(ValueRef::Fixed(Assignee::Agent(agent.into()))),
                project: None,
                harness: vec!["mock".into()],
                model: None,
                effort: None,
                output_schema: None,
                tier_ceiling: ToolTier::Write,
            },
            then,
        )
    }

    /// A notify → end workflow: the smallest one that runs without staff —
    /// in a goal's run or in the workspace alike.
    pub(crate) fn notify_workflow(name: &str) -> NewWorkflow {
        NewWorkflow {
            name: name.into(),
            description: "post and finish".into(),
            inputs: vec![],
            steps: vec![
                step(
                    "post",
                    StepKind::Notify {
                        scope: None,
                        template: "posted".into(),
                        mentions: vec![],
                        author: None,
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

    #[test]
    fn create_get_list_update_delete_workflow() {
        let (_dir, ws) = ws();
        let wf = ws
            .create_workflow(notify_workflow("Post"), WorkflowOrigin::Workspace)
            .unwrap();
        assert_eq!(wf.revision, 1);
        assert_eq!(wf.origin, WorkflowOrigin::Workspace);
        assert_eq!(wf.author, ws.owner_principal());
        assert_eq!(ws.get_workflow(wf.id).unwrap(), wf);
        assert_eq!(ws.list_workflows().unwrap(), vec![wf.clone()]);

        let mut edited = NewWorkflow::from(&wf);
        edited.name = "Post twice".into();
        let saved = ws.update_workflow(wf.id, edited, wf.revision).unwrap();
        assert_eq!(saved.name, "Post twice");
        assert_eq!(saved.revision, 2);
        assert_eq!(
            saved.origin,
            WorkflowOrigin::Workspace,
            "provenance is the store's"
        );
        assert_eq!(saved.author, wf.author);
        assert_eq!(saved.created_at, wf.created_at);
        assert_eq!(ws.get_workflow(wf.id).unwrap().revision, 2);

        ws.delete_workflow(wf.id).unwrap();
        assert!(matches!(
            ws.get_workflow(wf.id),
            Err(StoreError::WorkflowNotFound(_))
        ));
        assert!(ws.list_workflows().unwrap().is_empty());
    }

    #[test]
    fn an_invalid_workflow_is_refused_with_its_problems() {
        let (_dir, ws) = ws();
        let mut new = notify_workflow("Broken");
        new.steps[0].then.push(Flow::to(sid("nowhere")));
        new.steps.push(agent_step("ghost", "nobody", &[]));
        let err = ws
            .create_workflow(new.clone(), bisa_core::WorkflowOrigin::Workspace)
            .unwrap_err();
        match &err {
            StoreError::WorkflowInvalid(problems) => {
                let kinds: Vec<ProblemKind> = problems.iter().map(|p| p.kind).collect();
                assert!(kinds.contains(&ProblemKind::UnknownStep), "{kinds:?}");
                assert!(kinds.contains(&ProblemKind::UnknownAssignee), "{kinds:?}");
                assert!(kinds.contains(&ProblemKind::ManyStarts), "{kinds:?}");
            }
            other => panic!("{other:?}"),
        }
        assert!(err.to_string().contains("problems"), "{err}");
        assert!(
            ws.list_workflows().unwrap().is_empty(),
            "nothing was written"
        );

        // A draft save keeps the problems and the file.
        let wf = ws
            .create_workflow(
                notify_workflow("Draft"),
                bisa_core::WorkflowOrigin::Workspace,
            )
            .unwrap();
        let mut draft = NewWorkflow::from(&wf);
        draft.steps[0].then.push(Flow::to(sid("nowhere")));
        let (saved, problems) = ws
            .save_workflow_draft(wf.id, draft.clone(), wf.revision)
            .unwrap();
        assert_eq!(saved.revision, 2);
        assert!(!problems.is_empty());
        assert!(
            matches!(
                ws.update_workflow(wf.id, draft, saved.revision),
                Err(StoreError::WorkflowInvalid(_))
            ),
            "a real save refuses"
        );
    }

    /// The three refusals that keep a run from reading what is not there:
    /// a field the producer's schema does not promise, a step whose kind
    /// yields no output, and a producer not sure to have run when the reader
    /// is entered — a rework loop's head reading the reviewer.
    #[test]
    fn a_workflow_that_reads_what_is_not_promised_or_not_yet_there_is_refused() {
        let (_dir, ws) = ws();
        // The staff the steps name must exist, or every refusal below would
        // come with an `UnknownAssignee` beside it.
        ws.add_agent(crate::agents::NewAgent {
            name: "Developer".into(),
            harness: "mock".into(),
            system_prompt: "build".into(),
            ..Default::default()
        })
        .unwrap();
        let reading = |id: &str, instructions: &str, then: &[&str]| {
            step(
                id,
                StepKind::Agent {
                    instructions: instructions.into(),
                    assignee: Some(ValueRef::Fixed(Assignee::Agent("developer".into()))),
                    project: None,
                    harness: vec!["mock".into()],
                    model: None,
                    effort: None,
                    output_schema: None,
                    tier_ceiling: ToolTier::Write,
                },
                then,
            )
        };
        let promising = |mut s: Step, fields: &[&str]| {
            if let StepKind::Agent { output_schema, .. } = &mut s.kind {
                *output_schema = Some(serde_json::json!({ "type": "object", "required": fields }));
            }
            s
        };
        let kinds_of = |new: NewWorkflow| -> Vec<ProblemKind> {
            match ws.create_workflow(new, WorkflowOrigin::Workspace) {
                Err(StoreError::WorkflowInvalid(problems)) => {
                    problems.iter().map(|p| p.kind).collect()
                }
                other => panic!("{other:?}"),
            }
        };
        let with = |steps: Vec<Step>| NewWorkflow {
            name: "Reads".into(),
            description: String::new(),
            inputs: vec![],
            steps,
            tags: Tags::default(),
            decision_making: false,
        };

        // A field the schema does not require.
        let kinds = kinds_of(with(vec![
            promising(agent_step("review", "developer", &["fix"]), &["verdict"]),
            reading("fix", "Fix {steps.review.output.findings}", &[]),
        ]));
        assert_eq!(kinds, vec![ProblemKind::UnpromisedOutput], "{kinds:?}");

        // A human step yields an answer, not an output.
        let kinds = kinds_of(with(vec![
            step(
                "ask",
                StepKind::Human {
                    prompt: "?".into(),
                    options: vec![],
                    multi: false,
                    assignee: None,
                },
                &["fix"],
            ),
            reading("fix", "Fix {steps.ask.output.text}", &[]),
        ]));
        assert_eq!(kinds, vec![ProblemKind::NoSuchOutput], "{kinds:?}");

        // The loop's head cannot read what the loop's later steps produce.
        let kinds = kinds_of(with(vec![
            reading(
                "implement",
                "Address {steps.review.output.findings}",
                &["review"],
            ),
            promising(
                agent_step("review", "developer", &["verdict"]),
                &["verdict", "findings"],
            ),
            {
                let mut verdict = step(
                    "verdict",
                    StepKind::Decide {
                        rules: vec![bisa_core::Rule {
                            when: bisa_core::Condition::OutputEquals {
                                step: sid("review"),
                                path: "verdict".into(),
                                value: serde_json::json!("approve"),
                            },
                            branch: bisa_core::Branch::new("approve").unwrap(),
                        }],
                        otherwise: bisa_core::Branch::new("changes").unwrap(),
                        pick: bisa_core::Pick::First,
                    },
                    &[],
                );
                verdict.then = vec![
                    Flow::branch(sid("done"), bisa_core::Branch::new("approve").unwrap()),
                    Flow::branch(sid("implement"), bisa_core::Branch::new("changes").unwrap()),
                ];
                verdict
            },
            step(
                "done",
                StepKind::End {
                    finish: bisa_core::Finish::Done,
                },
                &[],
            ),
        ]));
        assert_eq!(kinds, vec![ProblemKind::NotAssured], "{kinds:?}");
        assert!(
            ws.list_workflows().unwrap().is_empty(),
            "nothing was written"
        );
    }

    #[test]
    fn update_with_stale_revision_is_a_revision_conflict() {
        let (_dir, ws) = ws();
        let wf = ws
            .create_workflow(notify_workflow("Shared"), WorkflowOrigin::Workspace)
            .unwrap();
        // Two editors open revision 1. The first saves.
        let mut first = NewWorkflow::from(&wf);
        first.description = "first".into();
        let saved = ws.update_workflow(wf.id, first, 1).unwrap();
        assert_eq!(saved.revision, 2);
        // The second still holds revision 1: refused by name, nothing written.
        let mut second = NewWorkflow::from(&wf);
        second.description = "second".into();
        let err = ws.update_workflow(wf.id, second.clone(), 1).unwrap_err();
        match &err {
            StoreError::RevisionConflict {
                kind,
                id,
                expected,
                actual,
            } => {
                assert_eq!(*kind, "workflow");
                assert_eq!(id, &wf.id.to_string());
                assert_eq!((*expected, *actual), (1, 2));
            }
            other => panic!("{other:?}"),
        }
        assert!(err.is_refusal());
        let stored = ws.get_workflow(wf.id).unwrap();
        assert_eq!(stored.description, "first");
        assert_eq!(stored.revision, 2);
        // Rebased on what is stored, it lands.
        let third = ws.update_workflow(wf.id, second, 2).unwrap();
        assert_eq!(third.revision, 3);
        assert_eq!(third.description, "second");
    }

    #[test]
    fn draft_save_with_stale_revision_is_a_revision_conflict() {
        let (_dir, ws) = ws();
        let wf = ws
            .create_workflow(notify_workflow("Drafted"), WorkflowOrigin::Workspace)
            .unwrap();
        let (saved, _) = ws
            .save_workflow_draft(wf.id, NewWorkflow::from(&wf), 1)
            .unwrap();
        assert_eq!(saved.revision, 2);
        assert!(matches!(
            ws.save_workflow_draft(wf.id, NewWorkflow::from(&wf), 1),
            Err(StoreError::RevisionConflict {
                expected: 1,
                actual: 2,
                ..
            })
        ));
        // A revision from the future is just as stale.
        assert!(matches!(
            ws.save_workflow_draft(wf.id, NewWorkflow::from(&wf), 9),
            Err(StoreError::RevisionConflict {
                expected: 9,
                actual: 2,
                ..
            })
        ));
        assert_eq!(ws.get_workflow(wf.id).unwrap().revision, 2);
    }

    #[test]
    fn a_draft_is_created_with_its_problems() {
        let (_dir, ws) = ws();
        // An empty canvas: no steps, so no start. A save refuses; a draft keeps.
        let empty = NewWorkflow {
            name: "New workflow".into(),
            ..NewWorkflow::default()
        };
        assert!(matches!(
            ws.create_workflow(empty.clone(), WorkflowOrigin::Workspace),
            Err(StoreError::WorkflowInvalid(_))
        ));
        let (wf, problems) = ws
            .create_workflow_draft(empty, WorkflowOrigin::Workspace)
            .unwrap();
        assert_eq!(wf.revision, 1);
        assert_eq!(wf.origin, WorkflowOrigin::Workspace);
        assert_eq!(
            problems.iter().map(|p| p.kind).collect::<Vec<_>>(),
            vec![ProblemKind::NoStart]
        );
        assert_eq!(ws.get_workflow(wf.id).unwrap(), wf, "the draft is stored");
        // The next draft save names revision 1.
        let (again, problems) = ws
            .save_workflow_draft(wf.id, notify_workflow("Now complete"), 1)
            .unwrap();
        assert_eq!(again.revision, 2);
        assert!(problems.is_empty());
        // A draft under a catalog origin still cannot install a slug twice.
        let (installed, _) = ws
            .create_workflow_draft(
                notify_workflow("Tpl"),
                WorkflowOrigin::Catalog { slug: "tpl".into() },
            )
            .unwrap();
        let err = ws
            .create_workflow_draft(
                notify_workflow("Tpl"),
                WorkflowOrigin::Catalog { slug: "tpl".into() },
            )
            .unwrap_err();
        assert!(err.to_string().contains(&installed.id.to_string()), "{err}");
    }

    #[test]
    fn unknown_key_in_new_workflow_is_refused() {
        let err = serde_json::from_value::<NewWorkflow>(serde_json::json!({
            "name": "x", "steps": [], "descripton": "typo"
        }))
        .unwrap_err()
        .to_string();
        assert!(err.contains("descripton"), "{err}");
        assert!(serde_json::from_value::<NewWorkflow>(serde_json::json!({
            "name": "x", "steps": []
        }))
        .is_ok());
    }

    #[test]
    fn validation_reports_bad_cron_and_bad_schema() {
        let (_dir, ws) = ws();
        let mut new = notify_workflow("Scheduled");
        new.steps.insert(
            0,
            step(
                "hold",
                StepKind::Wait {
                    until: bisa_core::WaitFor::Schedule {
                        cron: ValueRef::Fixed("every tuesday".into()),
                        tz: None,
                    },
                },
                &["post"],
            ),
        );
        let err = ws
            .create_workflow(new, WorkflowOrigin::Workspace)
            .unwrap_err();
        let StoreError::WorkflowInvalid(problems) = err else {
            panic!("{err:?}");
        };
        assert_eq!(
            problems.iter().map(|p| p.kind).collect::<Vec<_>>(),
            vec![ProblemKind::BadCron]
        );
        assert!(
            problems[0].text.to_string().contains("every tuesday"),
            "{}",
            problems[0].text
        );

        let mut new = notify_workflow("Checked");
        new.steps.insert(
            1,
            step(
                "shape",
                StepKind::Check {
                    check: bisa_core::CheckKind::Schema {
                        schema: serde_json::json!({ "type": "not-a-type" }),
                        of: Some(sid("post")),
                    },
                },
                &["end"],
            ),
        );
        new.steps[0].then = vec![Flow::to(sid("shape"))];
        let err = ws
            .create_workflow(new, WorkflowOrigin::Workspace)
            .unwrap_err();
        let StoreError::WorkflowInvalid(problems) = err else {
            panic!("{err:?}");
        };
        assert!(
            problems.iter().any(|p| p.kind == ProblemKind::BadSchema),
            "{problems:?}"
        );

        // A well-formed schedule and schema pass the same checks.
        let mut fine = notify_workflow("Fine");
        fine.steps.insert(
            0,
            step(
                "hold",
                StepKind::Wait {
                    until: bisa_core::WaitFor::Schedule {
                        cron: ValueRef::Fixed("0 9 * * 1-5".into()),
                        tz: Some("Europe/Paris".into()),
                    },
                },
                &["post"],
            ),
        );
        ws.create_workflow(fine, WorkflowOrigin::Workspace).unwrap();
    }

    #[test]
    fn validation_reports_spawn_cycle_across_library() {
        let (_dir, ws) = ws();
        let spawn = |id: &str, target: WorkflowId, then: &[&str]| {
            step(
                id,
                StepKind::Spawn {
                    statement_template: "child".into(),
                    workflow: Some(target),
                    assignees: vec![],
                    inputs: Default::default(),
                    wait: true,
                },
                then,
            )
        };
        // A spawns nothing yet; B spawns A.
        let a = ws
            .create_workflow(notify_workflow("A"), WorkflowOrigin::Workspace)
            .unwrap();
        let mut b_body = notify_workflow("B");
        b_body.steps.insert(0, spawn("child", a.id, &["post"]));
        let b = ws
            .create_workflow(b_body, WorkflowOrigin::Workspace)
            .unwrap();
        // Now A wants to spawn B: that closes the loop.
        let mut a_body = NewWorkflow::from(&a);
        a_body.steps.insert(0, spawn("child", b.id, &["post"]));
        let err = ws
            .update_workflow(a.id, a_body.clone(), a.revision)
            .unwrap_err();
        let StoreError::WorkflowInvalid(problems) = err else {
            panic!("{err:?}");
        };
        assert_eq!(
            problems.iter().map(|p| p.kind).collect::<Vec<_>>(),
            vec![ProblemKind::SpawnCycle]
        );
        assert_eq!(problems[0].step, Some(sid("child")));
        // Spawning a third workflow that spawns nothing is fine.
        let c = ws
            .create_workflow(notify_workflow("C"), WorkflowOrigin::Workspace)
            .unwrap();
        a_body.steps[0] = spawn("child", c.id, &["post"]);
        ws.update_workflow(a.id, a_body, a.revision).unwrap();
    }

    #[test]
    fn harness_names_are_checked_once_the_runtime_is_known() {
        let (_dir, ws) = ws();
        ws.add_agent(crate::agents::NewAgent {
            name: "Developer".into(),
            harness: "mock".into(),
            system_prompt: "build".into(),
            ..Default::default()
        })
        .unwrap();
        let mut new = notify_workflow("Staffed");
        new.steps
            .insert(0, agent_step("build", "developer", &["post"]));
        // No runtime described: the harness `mock` is not judged.
        let wf = ws
            .create_workflow(new.clone(), WorkflowOrigin::Workspace)
            .unwrap();
        assert!(ws.validate_workflow(&wf).unwrap().is_empty());

        // The engine describes a runtime without `mock`.
        ws.set_known_runtime(crate::workspace::KnownRuntime {
            harnesses: vec!["claude-code".into()],
            models: vec![("claude-code".into(), vec!["fable-5".into()])],
            effort_harnesses: vec!["claude-code".into()],
            topics: &[],
        });
        let problems = ws.validate_workflow(&wf).unwrap();
        assert_eq!(
            problems.iter().map(|p| p.kind).collect::<Vec<_>>(),
            vec![ProblemKind::UnknownHarness]
        );
        // With `mock` known, a pin on a model it does not list is the problem.
        ws.set_known_runtime(crate::workspace::KnownRuntime {
            harnesses: vec!["mock".into()],
            models: vec![("mock".into(), vec!["m1".into()])],
            effort_harnesses: vec![],
            topics: &[],
        });
        assert!(ws.validate_workflow(&wf).unwrap().is_empty());
        let mut pinned = NewWorkflow::from(&wf);
        if let StepKind::Agent { model, .. } = &mut pinned.steps[0].kind {
            *model = Some("zzz".into());
        }
        let err = ws.update_workflow(wf.id, pinned, wf.revision).unwrap_err();
        let StoreError::WorkflowInvalid(problems) = err else {
            panic!("{err:?}");
        };
        assert_eq!(
            problems.iter().map(|p| p.kind).collect::<Vec<_>>(),
            vec![ProblemKind::UnknownModel]
        );
    }

    #[test]
    fn an_effort_pin_is_checked_against_the_harnesses_that_take_one() {
        let (_dir, ws) = ws();
        ws.add_agent(crate::agents::NewAgent {
            name: "Developer".into(),
            harness: "mock".into(),
            system_prompt: "build".into(),
            ..Default::default()
        })
        .unwrap();
        let mut new = notify_workflow("Pinned");
        let mut build = agent_step("build", "developer", &["post"]);
        if let StepKind::Agent { effort, .. } = &mut build.kind {
            *effort = Some(bisa_core::EffortChoice::Xhigh);
        }
        new.steps.insert(0, build);
        // No runtime described: nobody can say which harness has the control.
        let wf = ws
            .create_workflow(new.clone(), WorkflowOrigin::Workspace)
            .unwrap();
        assert!(ws.validate_workflow(&wf).unwrap().is_empty());
        // The pin is kept as it was written.
        let kept = ws.get_workflow(wf.id).unwrap();
        assert!(matches!(
            kept.steps[0].kind,
            StepKind::Agent {
                effort: Some(bisa_core::EffortChoice::Xhigh),
                ..
            }
        ));

        // The engine describes `mock` as a harness with no effort control.
        ws.set_known_runtime(crate::workspace::KnownRuntime {
            harnesses: vec!["mock".into(), "claude-code".into()],
            models: vec![],
            effort_harnesses: vec!["claude-code".into()],
            topics: &[],
        });
        let problems = ws.validate_workflow(&wf).unwrap();
        assert_eq!(
            problems.iter().map(|p| p.kind).collect::<Vec<_>>(),
            vec![ProblemKind::UnsupportedEffort]
        );
        assert_eq!(problems[0].step, Some(sid("build")));

        // And then as one that has it: the same step is fine.
        ws.set_known_runtime(crate::workspace::KnownRuntime {
            harnesses: vec!["mock".into()],
            models: vec![],
            effort_harnesses: vec!["mock".into()],
            topics: &[],
        });
        assert!(ws.validate_workflow(&wf).unwrap().is_empty());
        assert_eq!(ws.known_runtime().effort_harnesses, ["mock"]);
    }

    #[test]
    fn a_workflow_a_goal_uses_cannot_be_deleted() {
        let (_dir, ws) = ws();
        let wf = ws
            .create_workflow(
                notify_workflow("Used"),
                bisa_core::WorkflowOrigin::Workspace,
            )
            .unwrap();
        let goal = ws.create_goal(NewGoal::captured("use it")).unwrap();
        ws.set_goal_workflow(goal.id, Some(wf.id)).unwrap();
        let err = ws.delete_workflow(wf.id).unwrap_err().to_string();
        assert!(err.contains("goal"), "{err}");
        assert!(err.contains("first"), "{err}");
        ws.set_goal_workflow(goal.id, None).unwrap();
        ws.delete_workflow(wf.id).unwrap();
    }

    #[test]
    fn workflows_survive_a_rebuild() {
        let (_dir, ws) = ws();
        let a = ws
            .create_workflow(notify_workflow("A"), bisa_core::WorkflowOrigin::Workspace)
            .unwrap();
        let mut b = notify_workflow("B");
        b.tags = Tags::new(["ops"]).unwrap();
        let b = ws
            .create_workflow(b, bisa_core::WorkflowOrigin::Workspace)
            .unwrap();
        ws.rebuild_index().unwrap();
        assert_eq!(ws.list_workflows().unwrap(), vec![a, b.clone()]);
        assert_eq!(
            ws.idx()
                .tags_of(TagEntity::Workflow, &b.id.to_string())
                .unwrap(),
            vec!["ops".to_string()]
        );
        assert!(ws
            .idx()
            .search("post and finish")
            .unwrap()
            .contains(&b.id.to_string()));
    }

    /// The engine names the origin; the library scope hides a goal's designs
    /// and the goal's scope is its designs alone.
    #[test]
    fn the_library_scope_hides_a_goals_designs_and_the_goal_scope_shows_them() {
        let (_dir, ws) = ws();
        let goal = ws.create_goal(NewGoal::captured("design me")).unwrap();
        let library = ws
            .create_workflow(notify_workflow("Ours"), WorkflowOrigin::Workspace)
            .unwrap();
        let design = ws
            .create_workflow(
                notify_workflow("For one goal"),
                WorkflowOrigin::Goal { goal: goal.id },
            )
            .unwrap();
        assert_eq!(design.origin, WorkflowOrigin::Goal { goal: goal.id });

        let lib = ws.list_workflows_in(WorkflowScope::Library).unwrap();
        assert!(lib.iter().any(|w| w.id == library.id));
        assert!(
            !lib.iter().any(|w| w.id == design.id),
            "a design is not the library's"
        );

        let goals_own = ws.list_workflows_in(WorkflowScope::Goal(goal.id)).unwrap();
        assert_eq!(
            goals_own.iter().map(|w| w.id).collect::<Vec<_>>(),
            vec![design.id]
        );

        let all = ws.list_workflows_in(WorkflowScope::All).unwrap();
        assert!(all.iter().any(|w| w.id == design.id));
    }

    /// Promote copies: a new id and a workspace origin in the library, the
    /// original untouched on its goal. A library workflow refuses.
    #[test]
    fn promote_copies_a_design_into_the_library_and_leaves_the_original() {
        let (_dir, ws) = ws();
        let goal = ws.create_goal(NewGoal::captured("design me")).unwrap();
        let design = ws
            .create_workflow(
                notify_workflow("Reusable"),
                WorkflowOrigin::Goal { goal: goal.id },
            )
            .unwrap();
        let copy = ws.promote_workflow(design.id).unwrap();
        assert_ne!(copy.id, design.id);
        assert_eq!(copy.origin, WorkflowOrigin::Workspace);
        assert_eq!(copy.revision, 1);
        assert_eq!(copy.name, design.name);
        assert_eq!(
            ws.get_workflow(design.id).unwrap().origin,
            WorkflowOrigin::Goal { goal: goal.id },
            "the original stays the goal's"
        );
        let err = ws.promote_workflow(copy.id).unwrap_err();
        assert!(
            matches!(&err, StoreError::AlreadyLibrary(name) if name == &copy.name),
            "{err:?}"
        );
        assert!(err.to_string().contains("already in the library"), "{err}");
        assert!(err.is_refusal());
        assert_eq!(
            ws.list_workflows_in(WorkflowScope::Library).unwrap().len(),
            1,
            "a second promotion writes nothing"
        );
    }

    /// Deleting a goal takes its designs with it — and only its designs.
    #[test]
    fn deleting_a_goal_removes_its_own_designs_and_nothing_else() {
        let (_dir, ws) = ws();
        let goal = ws.create_goal(NewGoal::captured("short-lived")).unwrap();
        let design = ws
            .create_workflow(
                notify_workflow("Its own"),
                WorkflowOrigin::Goal { goal: goal.id },
            )
            .unwrap();
        let library = ws
            .create_workflow(notify_workflow("Everyone's"), WorkflowOrigin::Workspace)
            .unwrap();
        ws.set_goal_workflow(goal.id, Some(design.id)).unwrap();
        ws.delete_goal(goal.id).unwrap();
        assert!(matches!(
            ws.get_workflow(design.id),
            Err(StoreError::WorkflowNotFound(_))
        ));
        ws.get_workflow(library.id).unwrap();
    }

    /// The filing rule: a design sits beside its goal's runs, a library
    /// workflow in `workflows/`; an edit keeps a design where it is.
    #[test]
    fn a_goals_design_is_filed_under_the_goal_and_a_library_workflow_under_workflows() {
        let (_dir, ws) = ws();
        let goal = ws.create_goal(NewGoal::captured("file me")).unwrap();
        let design = ws
            .create_workflow(
                notify_workflow("Design"),
                WorkflowOrigin::Goal { goal: goal.id },
            )
            .unwrap();
        let library = ws
            .create_workflow(notify_workflow("Library"), WorkflowOrigin::Workspace)
            .unwrap();
        let paths = ws.paths();
        assert!(paths.goal(goal.id).workflow_snapshot(design.id).is_file());
        assert!(!paths.library_workflow_snapshot(design.id).exists());
        assert!(paths.library_workflow_snapshot(library.id).is_file());
        assert!(!paths.goal(goal.id).workflow_snapshot(library.id).exists());
        assert_eq!(ws.get_workflow(design.id).unwrap(), design);
        assert_eq!(ws.get_workflow(library.id).unwrap(), library);

        let mut edited = NewWorkflow::from(&design);
        edited.description = "edited".into();
        let edited = ws
            .update_workflow(design.id, edited, design.revision)
            .unwrap();
        assert_eq!(edited.revision, 2);
        assert!(
            paths.goal(goal.id).workflow_snapshot(design.id).is_file(),
            "an edit keeps a design under its goal"
        );
        assert!(!paths.library_workflow_snapshot(design.id).exists());
    }

    #[test]
    fn promote_moves_a_copy_into_the_workflows_namespace() {
        let (_dir, ws) = ws();
        let goal = ws.create_goal(NewGoal::captured("promote me")).unwrap();
        let design = ws
            .create_workflow(
                notify_workflow("Reusable"),
                WorkflowOrigin::Goal { goal: goal.id },
            )
            .unwrap();
        let copy = ws.promote_workflow(design.id).unwrap();
        let paths = ws.paths();
        assert!(paths.library_workflow_snapshot(copy.id).is_file());
        assert!(
            paths.goal(goal.id).workflow_snapshot(design.id).is_file(),
            "the original stays under its goal"
        );
        let lib: Vec<_> = ws
            .list_workflows_in(WorkflowScope::Library)
            .unwrap()
            .into_iter()
            .map(|w| w.id)
            .collect();
        assert_eq!(lib, vec![copy.id]);
    }

    #[test]
    fn rebuild_finds_designs_under_their_goals() {
        let (_dir, ws) = ws();
        let goal = ws.create_goal(NewGoal::captured("rebuild me")).unwrap();
        let mut tagged = notify_workflow("Design");
        tagged.tags = Tags::new(["ops"]).unwrap();
        let design = ws
            .create_workflow(tagged, WorkflowOrigin::Goal { goal: goal.id })
            .unwrap();
        let library = ws
            .create_workflow(notify_workflow("Library"), WorkflowOrigin::Workspace)
            .unwrap();
        ws.rebuild_index().unwrap();
        assert_eq!(
            ws.list_workflows_in(WorkflowScope::Goal(goal.id)).unwrap(),
            vec![design.clone()]
        );
        assert_eq!(
            ws.list_workflows_in(WorkflowScope::Library).unwrap(),
            vec![library]
        );
        assert_eq!(
            ws.idx()
                .get_workflow(&design.id.to_string())
                .unwrap()
                .unwrap()
                .goal_id,
            Some(goal.id.to_string())
        );
        assert_eq!(
            ws.idx()
                .tags_of(TagEntity::Workflow, &design.id.to_string())
                .unwrap(),
            vec!["ops".to_string()]
        );
        assert_eq!(
            ws.get_workflow(design.id).unwrap(),
            design,
            "located through the rebuilt row"
        );
    }

    /// The folder takes the files; the rows and the refusal are the store's.
    #[test]
    fn deleting_a_goal_takes_its_designs_with_the_folder() {
        let (_dir, ws) = ws();
        let goal = ws.create_goal(NewGoal::captured("gone soon")).unwrap();
        let design = ws
            .create_workflow(
                notify_workflow("Its own"),
                WorkflowOrigin::Goal { goal: goal.id },
            )
            .unwrap();
        let library = ws
            .create_workflow(notify_workflow("Everyone's"), WorkflowOrigin::Workspace)
            .unwrap();
        ws.delete_goal(goal.id).unwrap();
        assert!(!ws.paths().goal(goal.id).dir().exists());
        assert!(matches!(
            ws.get_workflow(design.id),
            Err(StoreError::WorkflowNotFound(_))
        ));
        assert!(ws
            .idx()
            .get_workflow(&design.id.to_string())
            .unwrap()
            .is_none());
        assert_eq!(ws.get_workflow(library.id).unwrap(), library);
        ws.rebuild_index().unwrap();
        assert_eq!(ws.list_workflows().unwrap(), vec![library]);
    }

    // added by the coverage pass: workflows.rs

    // --- the bare lines of the workflows module ---

    /// The hosts listening with a workflow: a library workflow that is Off
    /// is no host; a goal listening with another workflow is not its; a
    /// goal row whose folder is gone, or whose id is not one, is skipped;
    /// a goal nobody may read is an I/O error.
    #[test]
    fn the_listening_hosts_of_a_workflow_are_the_goals_listening_with_it() {
        let (_d, ws) = ws();
        let wf = ws
            .create_workflow(notify_workflow("Lib"), WorkflowOrigin::Workspace)
            .unwrap();
        let other = ws
            .create_workflow(notify_workflow("Other"), WorkflowOrigin::Workspace)
            .unwrap();
        assert!(ws.listening_hosts_of(&wf).unwrap().is_empty());
        let listening = || {
            Some(bisa_core::Listening {
                inputs: Default::default(),
                budget: None,
                since: 1,
                paused: None,
            })
        };
        let mine = ws.create_goal(NewGoal::captured("mine")).unwrap();
        ws.set_goal_workflow(mine.id, Some(wf.id)).unwrap();
        ws.set_listening(
            &bisa_core::ListenerHost::Goal { goal: mine.id },
            listening(),
        )
        .unwrap();
        let theirs = ws.create_goal(NewGoal::captured("theirs")).unwrap();
        ws.set_goal_workflow(theirs.id, Some(other.id)).unwrap();
        ws.set_listening(
            &bisa_core::ListenerHost::Goal { goal: theirs.id },
            listening(),
        )
        .unwrap();
        assert_eq!(
            ws.listening_hosts_of(&wf).unwrap(),
            vec![bisa_core::ListenerHost::Goal { goal: mine.id }]
        );
        // The other goal's folder is gone: the row is skipped.
        let gone_dir = ws.paths.goal(theirs.id).dir().to_path_buf();
        let aside = gone_dir.with_file_name("aside");
        std::fs::rename(&gone_dir, &aside).unwrap();
        assert_eq!(ws.listening_hosts_of(&wf).unwrap().len(), 1);
        std::fs::rename(&aside, &gone_dir).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&gone_dir, std::fs::Permissions::from_mode(0o000)).unwrap();
            let err = ws.listening_hosts_of(&wf);
            std::fs::set_permissions(&gone_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
            assert!(matches!(err, Err(StoreError::Io { .. })), "{err:?}");
        }
        ws.idx()
            .execute_for_test(&format!(
                "UPDATE goals SET id = 'not-a-goal' WHERE id = '{}'",
                theirs.id
            ))
            .unwrap();
        assert_eq!(ws.listening_hosts_of(&wf).unwrap().len(), 1);
    }

    /// A workflow row that names no workflow, or a goal that is none, is
    /// refused by name wherever a list reads it — the one row a rebuild
    /// repairs.
    #[test]
    fn workflow_rows_that_name_no_workflow_or_no_goal_are_refused_by_name() {
        let (_d, ws) = ws();
        let lib = ws
            .create_workflow(notify_workflow("Lib"), WorkflowOrigin::Workspace)
            .unwrap();
        let goal = ws.create_goal(NewGoal::captured("designed")).unwrap();
        let design = ws
            .create_workflow(
                notify_workflow("Design"),
                WorkflowOrigin::Goal { goal: goal.id },
            )
            .unwrap();
        let other_design = ws
            .create_workflow(
                notify_workflow("Other design"),
                WorkflowOrigin::Goal { goal: goal.id },
            )
            .unwrap();
        let put_away = ws
            .create_workflow(notify_workflow("Old"), WorkflowOrigin::Workspace)
            .unwrap();
        ws.set_workflow_archived(put_away.id, true).unwrap();
        ws.install(crate::catalog::CatalogKind::Workflow, "bug-fix")
            .unwrap();
        let bad = |err: StoreError| assert!(matches!(&err, StoreError::Invalid(_)), "{err:?}");
        ws.idx()
            .execute_for_test(&format!(
                "UPDATE workflows SET id = 'bad-lib' WHERE id = '{}'",
                lib.id
            ))
            .unwrap();
        bad(ws.list_workflows().unwrap_err());
        let changed = ws
            .idx()
            .execute_for_test(&format!(
                "UPDATE workflows SET goal_id = 'not-a-goal' WHERE id = '{}'",
                design.id
            ))
            .unwrap();
        assert_eq!(changed, 1);
        bad(ws.get_workflow(design.id).unwrap_err());
        ws.idx()
            .execute_for_test(&format!(
                "UPDATE workflows SET id = 'bad-design' WHERE id = '{}'",
                other_design.id
            ))
            .unwrap();
        bad(ws.delete_goal(goal.id).unwrap_err());
        ws.idx()
            .execute_for_test(&format!(
                "UPDATE workflows SET id = 'bad-archived' WHERE id = '{}'",
                put_away.id
            ))
            .unwrap();
        bad(ws
            .list_archived_workflows_in(WorkflowScope::All)
            .unwrap_err());
        ws.idx()
            .execute_for_test("UPDATE workflows SET id = 'bad-slug' WHERE catalog_slug = 'bug-fix'")
            .unwrap();
        bad(ws.workflow_for_slug("bug-fix").unwrap_err());
        ws.rebuild_index().unwrap();
        assert!(ws.get_workflow(lib.id).is_ok());
        assert!(ws.workflow_for_slug("bug-fix").unwrap().is_some());
    }

    /// A workflow filed under a goal whose origin says it is the library's
    /// — a peer's mistake — is skipped by the rebuild, never indexed twice.
    #[test]
    fn a_workflow_filed_in_the_wrong_namespace_is_skipped_by_the_rebuild() {
        let (_d, ws) = ws();
        let lib = ws
            .create_workflow(notify_workflow("Lib"), WorkflowOrigin::Workspace)
            .unwrap();
        let goal = ws.create_goal(NewGoal::captured("host")).unwrap();
        let event = ws
            .snapshots
            .get_raw(Paths::NS_WORKFLOWS, KIND_WORKFLOW, &lib.id.to_string())
            .unwrap()
            .unwrap();
        assert!(ws
            .snapshots
            .apply_remote(&Paths::ns_goal(goal.id), &event)
            .unwrap());
        ws.rebuild_index().unwrap();
        assert_eq!(
            ws.list_workflows()
                .unwrap()
                .into_iter()
                .map(|w| w.id)
                .collect::<Vec<_>>(),
            vec![lib.id]
        );
        assert_eq!(ws.get_workflow(lib.id).unwrap().origin, lib.origin);
    }
}
