//! Retiring a goal or a workflow — archiving it or deleting it — with what
//! came from it, on one plan stated up front.
//!
//! A **preview** ([`preview_goal`], [`preview_workflow`]) is the facts a
//! person weighs: the sessions that will stop, the designs that go or stay,
//! the projects *born of* the thing (a goal's, or a workflow's steps') with
//! what each holds, the projects merely attached, and — for a workflow —
//! what still uses it, its runs of the workspace that are going (retired
//! with it, whatever its fate) and how many it has in all (kept under an
//! archived workflow, gone with a deleted one), and the refusal a deletion
//! would meet. A **plan** ([`GoalPlan`], [`WorkflowPlan`]) is the choice:
//! the thing's fate, the born projects' fate, and whether a deleted
//! project's managed folder goes to the Trash. [`retire_goal`] and
//! [`retire_workflow`] carry it out in one order: refuse first, stop the
//! work — a goal's run closed with it, a workflow's runs of the workspace
//! retired — **wait for the sessions to end**, settle the projects, then the
//! thing itself.
//!
//! What never happens here: a refusal after something was stopped; a
//! project born elsewhere is only ever detached; an adopted folder is never
//! moved; a workflow something uses is never deleted (I15 — it is archived
//! instead); nothing is decided by a default.

use crate::events::{EngineEvent, EnginePayload};
use crate::sessions::{self, Scope};
use crate::{ops, projects, EngineError, Inner};
use bisa_core::{
    CancelCause, ClosureReason, GoalId, Project, ProjectId, ProjectRoot, RunId, RunStatus,
    WorkflowId, WorkflowRun, WorkstreamId,
};
use bisa_store::{Reference, WorkflowScope, WorkstreamFilter};
pub use projects::Deleted;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;

/// What becomes of a thing: left as it is, put away, or gone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Fate {
    Keep,
    Archive,
    Delete,
}

/// The choice for a goal: its own fate (archive or delete — never keep), the
/// fate of the projects born of it, and whether a deleted project's managed
/// folder goes to the Trash.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GoalPlan {
    pub goal: Fate,
    pub projects: Fate,
    #[serde(default)]
    pub tree: bool,
}

/// The choice for a workflow, the same shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkflowPlan {
    pub workflow: Fate,
    pub projects: Fate,
    #[serde(default)]
    pub tree: bool,
}

/// A project as the preview lists it: enough to weigh its fate.
#[derive(Clone, Debug, PartialEq, Serialize, schemars::JsonSchema)]
pub struct ProjectFacts {
    pub id: ProjectId,
    pub slug: String,
    pub name: String,
    /// An adopted folder: its records may go, its folder never moves.
    pub adopted: bool,
    pub workstreams: usize,
    /// Its workstreams by id — the desktop counts the terminals it has
    /// rooted there, which the node never knows.
    pub workstream_ids: Vec<WorkstreamId>,
    /// Live sessions in its workstreams.
    pub sessions: usize,
    pub archived: bool,
}

/// A run that is going — a goal's, or a workflow's in the workspace — as the
/// preview names it.
#[derive(Clone, Debug, PartialEq, Serialize, schemars::JsonSchema)]
pub struct RunFacts {
    pub id: RunId,
    pub status: RunStatus,
    /// Steps running or waiting right now — cancelled with the run.
    pub live_steps: usize,
}

impl RunFacts {
    fn of(run: &WorkflowRun) -> Self {
        RunFacts {
            id: run.id,
            status: run.status(),
            live_steps: run.live_steps().len(),
        }
    }
}

/// What retiring would touch. The node mirrors it as a DTO (a `Reference`
/// is the store's word, not a wire shape).
#[derive(Clone, Debug, PartialEq)]
pub struct Retirement {
    /// Live engine sessions on the thing itself — aborted by a retirement
    /// (a goal's; a workflow's runs of the workspace's).
    pub agents: usize,
    /// Live interactive sessions on it — a harness a person opened in a
    /// terminal, terminated by the desktop when its row ends.
    pub harnesses: usize,
    /// The unfinished run on a goal, cancelled by a retirement.
    pub run: Option<RunFacts>,
    /// A workflow's runs of the workspace that are going — retired
    /// (cancelled) by a retirement, whatever its fate.
    pub runs: Vec<RunFacts>,
    /// How many runs of the workspace a workflow has in all: kept under an
    /// archived workflow, deleted with a deleted one.
    pub history: usize,
    /// Why a deletion would be refused, said before anything stops: a goal
    /// whose design something else still uses. Archive stays available.
    pub refusal: Option<String>,
    /// A goal's own designs — they go with a deleted goal and stay under an archived one.
    pub designs: usize,
    /// What still uses a workflow: while non-empty its deletion is refused.
    pub used_by: Vec<Reference>,
    /// The projects born of the thing — the ones the plan's `projects` fate applies to.
    pub projects_born: Vec<ProjectFacts>,
    /// The projects merely attached to a goal: detached on delete, left alone on archive, never deleted.
    pub projects_attached: Vec<ProjectFacts>,
}

/// What retiring did.
#[derive(Clone, Debug, PartialEq, Serialize, schemars::JsonSchema)]
pub struct Retired {
    pub stopped_sessions: usize,
    /// Rows still live when the wait ran out: their process was told and
    /// their row is aborted; the retirement did not wait longer.
    pub unsettled_sessions: usize,
    pub projects: Vec<ProjectFate>,
    /// The workstreams whose records went or were put away — the desktop closes the shells rooted at them.
    pub workstreams_retired: Vec<WorkstreamId>,
}

#[derive(Clone, Debug, PartialEq, Serialize, schemars::JsonSchema)]
pub struct ProjectFate {
    pub id: ProjectId,
    pub fate: Fate,
}

fn facts(inner: &Inner, p: &Project) -> Result<ProjectFacts, EngineError> {
    let workstreams = inner.ws.list_workstreams(WorkstreamFilter::Project(p.id))?;
    let wids: HashSet<WorkstreamId> = workstreams.iter().map(|w| w.id).collect();
    let sessions = inner
        .presence
        .snapshot()
        .iter()
        .filter(|s| {
            s.state.is_live()
                && (s.project == Some(p.id) || s.workstream.is_some_and(|w| wids.contains(&w)))
        })
        .count();
    Ok(ProjectFacts {
        id: p.id,
        slug: p.slug.to_string(),
        name: p.name.clone(),
        adopted: matches!(p.root, ProjectRoot::External { .. }),
        workstreams: workstreams.len(),
        workstream_ids: workstreams.iter().map(|w| w.id).collect(),
        sessions,
        archived: p.is_archived(),
    })
}

fn all_facts(inner: &Inner, projects: &[Project]) -> Result<Vec<ProjectFacts>, EngineError> {
    projects.iter().map(|p| facts(inner, p)).collect()
}

/// What retiring a goal would touch.
pub fn preview_goal(inner: &Arc<Inner>, goal: GoalId) -> Result<Retirement, EngineError> {
    inner.ws.get_goal(goal)?;
    let rows = inner.presence.snapshot();
    let live = rows
        .iter()
        .filter(|s| s.state.is_live() && s.goal == Some(goal));
    let harnesses = live
        .clone()
        .filter(|s| s.kind == crate::registry::SessionKind::Terminal)
        .count();
    let agents = live.count() - harnesses;
    let run = inner
        .ws
        .get_current_run(goal)?
        .filter(|r| !r.is_finished())
        .map(|r| RunFacts::of(&r));
    let refusal = inner.ws.goal_deletable(goal).err().map(|e| e.to_string());
    let designs = inner.ws.list_workflows_in(WorkflowScope::Goal(goal))?.len()
        + inner
            .ws
            .list_archived_workflows_in(WorkflowScope::Goal(goal))?
            .len();
    Ok(Retirement {
        agents,
        harnesses,
        run,
        runs: vec![],
        history: 0,
        refusal,
        designs,
        used_by: vec![],
        projects_born: all_facts(inner, &inner.ws.projects_born_of_goal(goal)?)?,
        projects_attached: all_facts(inner, &inner.ws.projects_attached_only(goal)?)?,
    })
}

/// What retiring a workflow would touch.
pub fn preview_workflow(
    inner: &Arc<Inner>,
    workflow: WorkflowId,
) -> Result<Retirement, EngineError> {
    inner.ws.get_workflow(workflow)?;
    let used_by = inner
        .ws
        .usage_of(bisa_store::UsageKind::Workflow, &workflow.to_string())?
        .as_slice()
        .to_vec();
    let history = inner.ws.list_workflow_runs(workflow)?;
    let live: Vec<&WorkflowRun> = history.iter().filter(|r| r.is_live()).collect();
    let live_ids: HashSet<RunId> = live.iter().map(|r| r.id).collect();
    let agents = inner
        .presence
        .snapshot()
        .iter()
        .filter(|s| s.state.is_live() && s.run.is_some_and(|r| live_ids.contains(&r)))
        .count();
    Ok(Retirement {
        agents,
        harnesses: 0,
        run: None,
        runs: live.into_iter().map(RunFacts::of).collect(),
        history: history.len(),
        refusal: None,
        designs: 0,
        used_by,
        projects_born: all_facts(inner, &inner.ws.projects_born_of_workflow(workflow)?)?,
        projects_attached: vec![],
    })
}

/// The workstreams of every project a fate would put away or delete.
fn workstreams_of(
    inner: &Inner,
    projects: &[Project],
) -> Result<HashSet<WorkstreamId>, EngineError> {
    let mut out = HashSet::new();
    for p in projects {
        for w in inner.ws.list_workstreams(WorkstreamFilter::Project(p.id))? {
            out.insert(w.id);
        }
    }
    Ok(out)
}

/// Settle the projects born of a thing, by the plan's fate. Answers what
/// became of each and the workstreams that were put away or forgotten.
async fn settle_projects(
    inner: &Arc<Inner>,
    born: Vec<Project>,
    fate: Fate,
    tree: bool,
) -> Result<(Vec<ProjectFate>, Vec<WorkstreamId>), EngineError> {
    let mut fates = Vec::with_capacity(born.len());
    let mut retired = Vec::new();
    for p in born {
        match fate {
            Fate::Keep => {}
            Fate::Archive => {
                if !p.is_archived() {
                    for w in inner.ws.list_workstreams(WorkstreamFilter::Project(p.id))? {
                        retired.push(w.id);
                    }
                    projects::archive(inner, p.id, true)?;
                }
            }
            Fate::Delete => {
                for w in inner.ws.list_workstreams(WorkstreamFilter::Project(p.id))? {
                    retired.push(w.id);
                }
                // An adopted folder never moves, whatever the plan said of trees.
                let tree = tree && !matches!(p.root, ProjectRoot::External { .. });
                projects::delete(inner, p.id, tree).await?;
            }
        }
        fates.push(ProjectFate { id: p.id, fate });
    }
    Ok((fates, retired))
}

/// Retire a goal on the plan: refuse first, stop its work, close it if
/// open, wait for the sessions to end, settle the projects born of it, then
/// archive or delete it.
pub async fn retire_goal(
    inner: &Arc<Inner>,
    goal: GoalId,
    plan: GoalPlan,
) -> Result<Retired, EngineError> {
    if plan.goal == Fate::Keep {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-goal-retired-archiving-deleting-keep-not-fate"
        )));
    }
    let g = inner.ws.get_goal(goal)?;
    // Every refusal before anything stops: a goal closed for a deletion that
    // then refuses is a goal nobody asked to close.
    if plan.goal == Fate::Delete {
        inner.ws.goal_deletable(goal)?;
    }
    let born = inner.ws.projects_born_of_goal(goal)?;
    // The sessions first: on the goal, and in the workstreams of the projects the plan touches.
    let touched = if plan.projects == Fate::Keep {
        HashSet::new()
    } else {
        workstreams_of(inner, &born)?
    };
    let stopped = sessions::stop_for(inner, Scope::Goal(goal), &touched);
    if !g.is_closed() {
        ops::close_goal(inner, goal, ClosureReason::Abandoned { rationale: None })?;
    }
    // Then the wait: the processes are told, and the folder goes only once
    // the rows are no longer live — or the bound passed and it is said.
    let settled = sessions::await_stopped(
        inner,
        Scope::Goal(goal),
        &touched,
        stopped,
        sessions::STOP_DEADLINE,
    )
    .await;
    let (projects, workstreams_retired) =
        settle_projects(inner, born, plan.projects, plan.tree).await?;
    match plan.goal {
        Fate::Archive => {
            inner.ws.set_goal_archived(goal, true)?;
            inner.emit(EngineEvent::scoped(
                goal,
                None,
                EnginePayload::GoalArchived {
                    goal,
                    archived: true,
                },
            ));
        }
        Fate::Delete => {
            ops::delete_goal(inner, goal)?;
            // Scoped to the goal: the screen open on it hears it and leaves.
            inner.emit(EngineEvent::scoped(
                goal,
                None,
                EnginePayload::GoalDeleted { goal },
            ));
        }
        Fate::Keep => {
            return Err(EngineError::Invalid(bisa_core::text!(
                "error-engine-invalid-goal-retired-archiving-deleting-keep-not-fate"
            )))
        }
    }
    // One line for the whole act: what went, and what was still running when it did.
    tracing::info!(
        %goal,
        fate = ?plan.goal,
        projects = projects.len(),
        workstreams = workstreams_retired.len(),
        stopped_sessions = settled.stopped,
        "a goal was retired"
    );
    if settled.still_live > 0 {
        tracing::warn!(%goal, still_live = settled.still_live, "a goal was retired while sessions of it had not ended within the deadline");
    }
    Ok(Retired {
        stopped_sessions: settled.stopped,
        unsettled_sessions: settled.still_live,
        projects,
        workstreams_retired,
    })
}

/// Take an archived goal back out. It stays closed.
pub fn unarchive_goal(inner: &Arc<Inner>, goal: GoalId) -> Result<bisa_core::Goal, EngineError> {
    let g = inner.ws.set_goal_archived(goal, false)?;
    inner.emit(EngineEvent::scoped(
        goal,
        None,
        EnginePayload::GoalArchived {
            goal,
            archived: false,
        },
    ));
    Ok(g)
}

/// Retire a workflow on the plan. Deletion is refused while a goal, another
/// workflow's spawn step or run start uses it (I15) — the preview says so,
/// and archive is the fate offered in its place. It stops listening first,
/// whatever its fate: nothing its events start slips in while it goes. Its
/// runs of the workspace that are going are retired next — cancelled (cause
/// *retired*), their sessions ended and waited for — whatever its fate
/// (I50); an archived workflow keeps its runs as history, a deleted one
/// takes them with it.
pub async fn retire_workflow(
    inner: &Arc<Inner>,
    workflow: WorkflowId,
    plan: WorkflowPlan,
) -> Result<Retired, EngineError> {
    if plan.workflow == Fate::Keep {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-workflow-retired-archiving-deleting-keep-not-fate"
        )));
    }
    inner.ws.get_workflow(workflow)?;
    if plan.workflow == Fate::Delete {
        let usage = inner
            .ws
            .usage_of(bisa_store::UsageKind::Workflow, &workflow.to_string())?;
        if !usage.as_slice().is_empty() {
            return Err(EngineError::Invalid(bisa_core::text!(
                "error-engine-invalid-workflow-used-archive-point-them-elsewhere-first",
                workflow = workflow.to_string(),
                a0 = (usage.describe()).to_string()
            )));
        }
    }
    crate::listen::turn::turn_off(inner, bisa_core::ListenerHost::Workspace { workflow })?;
    let born = inner.ws.projects_born_of_workflow(workflow)?;
    let touched = if plan.projects == Fate::Keep {
        HashSet::new()
    } else {
        workstreams_of(inner, &born)?
    };
    let mut stopped_sessions = 0;
    let mut unsettled_sessions = 0;
    for run in inner.ws.live_workspace_runs(Some(workflow))? {
        let (_, settled) = ops::end_workspace_run(inner, run.id, CancelCause::Retired).await?;
        stopped_sessions += settled.stopped;
        unsettled_sessions += settled.still_live;
    }
    for p in &born {
        if plan.projects != Fate::Keep {
            let stopped = sessions::stop_for(inner, Scope::Project(p.id), &touched);
            let settled = sessions::await_stopped(
                inner,
                Scope::Project(p.id),
                &touched,
                stopped,
                sessions::STOP_DEADLINE,
            )
            .await;
            stopped_sessions += settled.stopped;
            unsettled_sessions += settled.still_live;
        }
    }
    let (projects, workstreams_retired) =
        settle_projects(inner, born, plan.projects, plan.tree).await?;
    match plan.workflow {
        Fate::Archive => {
            ops::archive_workflow(inner, workflow, true)?;
        }
        Fate::Delete => ops::delete_workflow(inner, workflow)?,
        Fate::Keep => {
            return Err(EngineError::Invalid(bisa_core::text!(
                "error-engine-invalid-workflow-retired-archiving-deleting-keep-not-fate"
            )))
        }
    }
    Ok(Retired {
        stopped_sessions,
        unsettled_sessions,
        projects,
        workstreams_retired,
    })
}
