//! Workflow routes: the library a designer edits, a goal picks from and the
//! workspace runs.
//!
//! A save — `POST` (create) and `PUT` (edit) alike — answers with the
//! definition's problems rather than refusing them: a half-connected graph is
//! the normal state between two keystrokes, an empty canvas is how a new
//! workflow begins, and the designer shows the list. What *is* refused is a
//! start: `POST /workflows/{wfid}/runs`, `POST /goals/{id}/run` and an
//! adoption validate again before anything runs. (The catalog installer and
//! a proposal still refuse problems, through the store's own creating path.)
//!
//! **A run of the workspace** is a workflow's own: *Run…* starts one at once,
//! beside any other of it, with no goal behind it — by hand, or as a test
//! run of one of its event starts; the workflow's *Stop* and *Restart* act
//! on those alone — a goal's run of the same workflow is its goal's. What
//! hears its start events is `listening.rs`'s; its row says where it stands
//! (`listening`, `starts`, `event_only`, `listening_needs`).
//!
//! **One revision rule.** A `PUT` names the `revision` it was edited from;
//! the store's compare-and-swap refuses a stale one with a 409 whose message
//! names both revisions, and nothing is written. There is no handler-side
//! check to drift from it.

use crate::dto::*;
use crate::route_docs::RouteDoc;
use crate::tags::{parse_tags, TagFilter};
use crate::{bad_request, conflict, not_found, ApiError, Shared};
use axum::extract::{Path as AxPath, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use bisa_core::tags::TagEntity;
use bisa_core::{GoalId, ListenerHost, RunScope, WayIn, Workflow, WorkflowId, WorkflowOrigin};
use bisa_engine::retire::{Fate, WorkflowPlan};
use bisa_store::{NewWorkflow, UsageKind, WorkflowScope, Workspace};
use serde_json::json;
use std::str::FromStr as _;

pub(crate) fn routes() -> Router<Shared> {
    Router::new()
        .route("/workflows", get(list).post(create))
        .route("/workflows/validate", post(validate))
        .route("/workflows/{wfid}/retirement", get(retirement))
        .route("/workflows/{wfid}/retire", post(retire))
        .route("/workflows/{wfid}/archive", post(archive))
        .route("/workflows/{wfid}", get(detail).put(save).delete(remove))
        .route("/workflows/{wfid}/promote", post(promote))
        .route("/workflows/{wfid}/runs", get(runs).post(start_run))
        .route("/workflows/{wfid}/stop", post(stop))
        .route("/workflows/{wfid}/restart", post(restart))
}

/// A workflow named by id or by the catalog slug it was installed under.
/// A slug that is not installed is a 404 that says so: installing is
/// `POST /catalog/install`, a separate decision.
pub(crate) fn resolve_workflow(state: &Shared, raw: &str) -> Result<Workflow, ApiError> {
    let ws = state.engine.workspace();
    if let Ok(id) = WorkflowId::from_str(raw) {
        return ws.get_workflow(id).map_err(|_| {
            not_found(bisa_core::text!(
                "error-node-workflows-workflow-not-found",
                id = id.to_string()
            ))
        });
    }
    match ws.workflow_for_slug(raw)? {
        Some(wf) => Ok(wf),
        None => Err(not_found(bisa_core::text!(
            "error-node-workflows-no-installed-workflow-named-install-template-first",
            raw = format!("{raw:?}")
        ))),
    }
}

/// The store's creation shape from the wire's.
pub(crate) fn draft_of(body: NewWorkflowBody) -> Result<NewWorkflow, ApiError> {
    Ok(NewWorkflow {
        name: body.name,
        description: body.description,
        inputs: body.inputs,
        steps: body.steps,
        tags: parse_tags(&body.tags)?,
        decision_making: body.decision_making,
    })
}

pub(crate) fn row(state: &Shared, wf: Workflow) -> Result<WorkflowRow, ApiError> {
    Ok(row_of(state.engine.workspace(), wf)?)
}

/// A workflow as its row reads in `ws` — the one place the row is built: the
/// definition with its problems, its runs of the workspace, who holds it, and
/// where it stands as a listener. Public: the command line answers a toggle
/// with the same row when it reads the workspace itself.
pub fn row_of(ws: &Workspace, wf: Workflow) -> Result<WorkflowRow, bisa_store::StoreError> {
    let problems = ws.validate_workflow(&wf)?;
    // What stops it running in the workspace beyond its own problems: a step
    // that reads the goal it would not have. The budget is not read.
    let workspace_problems = wf.scope_problems(&RunScope::Workspace {
        budget: Default::default(),
    });
    let runs = WorkflowRuns::of(&ws.list_workflow_runs(wf.id)?, |run| {
        crate::runs::start_detail(ws, run)
    });
    let used_by = ws
        .usage_of(UsageKind::Workflow, &wf.id.to_string())?
        .as_slice()
        .iter()
        .cloned()
        .map(ReferenceDto::from)
        .collect();
    // Where it stands as a listener: On or Off, and what turning it On asks.
    let listening = ws.listening(&ListenerHost::Workspace { workflow: wf.id })?;
    Ok(WorkflowRow {
        starts: StartSummary::list(&wf, listening.as_ref()),
        event_only: wf.is_event_only(),
        listening_needs: listening_needs(&wf),
        listening,
        workflow: wf,
        problems,
        workspace_problems,
        runs,
        used_by,
    })
}

/// The workflow a route names, or its 404.
pub(crate) fn existing(state: &Shared, id: WorkflowId) -> Result<Workflow, ApiError> {
    state.engine.workspace().get_workflow(id).map_err(|_| {
        not_found(bisa_core::text!(
            "error-node-workflows-workflow-not-found",
            id = id.to_string()
        ))
    })
}

#[derive(serde::Deserialize, Default)]
struct ListQuery {
    /// `library` (default) hides every goal's designs; `all` shows them too.
    #[serde(default)]
    scope: Option<String>,
    /// One goal's designs instead.
    #[serde(default)]
    goal: Option<String>,
    /// Include the workflows put away — hidden unless asked.
    #[serde(default)]
    archived: bool,
}

/// The library (`?scope=library`, the default), everything (`?scope=all`),
/// or one goal's designs (`?goal=<id>`); `?tag=` filters any of them.
async fn list(
    State(state): State<Shared>,
    filter: TagFilter,
    crate::Query(q): crate::Query<ListQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let ws = state.engine.workspace();
    let scope = match (&q.goal, q.scope.as_deref()) {
        (Some(goal), _) => WorkflowScope::Goal(goal.parse::<GoalId>().map_err(|_| {
            bad_request(bisa_core::text!(
                "error-node-workflows-not-goal-id",
                goal = format!("{goal:?}")
            ))
        })?),
        (None, Some("all")) => WorkflowScope::All,
        (None, None | Some("library")) => WorkflowScope::Library,
        (None, Some(other)) => {
            return Err(bad_request(bisa_core::text!(
                "error-node-workflows-unknown-scope-expected-library-all",
                other = format!("{other:?}")
            )))
        }
    };
    let mut all = ws.list_workflows_in(scope)?;
    if q.archived {
        all.extend(ws.list_archived_workflows_in(scope)?);
    }
    let workflows = filter.apply(ws, TagEntity::Workflow, all, |w| w.id.to_string())?;
    let mut rows = Vec::with_capacity(workflows.len());
    for wf in workflows {
        rows.push(row(&state, wf)?);
    }
    Ok(Json(json!({"workflows": rows})))
}

/// Record a new local workflow as a draft: it is kept with its problems, and
/// they come back beside it. A start validates again.
async fn create(
    State(state): State<Shared>,
    crate::Body(body): crate::Body<NewWorkflowBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let (wf, problems) = state.engine.create_workflow_draft(draft_of(body)?)?;
    Ok(Json(json!({"workflow": wf, "problems": problems})))
}

/// Every problem a definition has, without recording it.
async fn validate(
    State(state): State<Shared>,
    crate::Body(body): crate::Body<NewWorkflowBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let ws = state.engine.workspace();
    let draft = draft_of(body)?;
    let wf = Workflow {
        id: WorkflowId::from_ulid(ulid::Ulid::from_datetime(std::time::SystemTime::now())),
        name: draft.name,
        description: draft.description,
        inputs: draft.inputs,
        steps: draft.steps,
        origin: WorkflowOrigin::Workspace,
        author: ws.owner_principal(),
        tags: draft.tags,
        revision: 0,
        archived: None,
        decision_making: draft.decision_making,
        created_at: 0,
    };
    let problems = state.engine.validate_workflow(&wf)?;
    Ok(Json(json!({"problems": problems})))
}

/// Copy a goal's design into the library. 409 for a library workflow.
async fn promote(
    State(state): State<Shared>,
    AxPath(wfid): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_wfid(&wfid)?;
    let copy = state.engine.promote_workflow(id)?;
    Ok(Json(json!({"workflow": copy})))
}

pub(crate) fn parse_wfid(s: &str) -> Result<WorkflowId, ApiError> {
    WorkflowId::from_str(s).map_err(|_| {
        bad_request(bisa_core::text!(
            "error-node-workflows-not-workflow-id",
            s = format!("{s:?}")
        ))
    })
}

/// One workflow, flat: `{workflow, problems, used_by}`.
async fn detail(
    State(state): State<Shared>,
    AxPath(wfid): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_wfid(&wfid)?;
    let wf = state.engine.workspace().get_workflow(id).map_err(|_| {
        not_found(bisa_core::text!(
            "error-node-workflows-workflow-not-found",
            id = id.to_string()
        ))
    })?;
    Ok(Json(serde_json::to_value(row(&state, wf)?)?))
}

/// Save an edited definition as its next revision. The body is the
/// definition plus the `revision` it was edited from; a stored copy that has
/// moved is a 409 from the store's compare-and-swap, so two designers cannot
/// silently overwrite each other. The problems come back rather than
/// refusing the save.
async fn save(
    State(state): State<Shared>,
    AxPath(wfid): AxPath<String>,
    crate::Body(body): crate::Body<PutWorkflowBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_wfid(&wfid)?;
    state.engine.workspace().get_workflow(id).map_err(|_| {
        not_found(bisa_core::text!(
            "error-node-workflows-workflow-not-found",
            id = id.to_string()
        ))
    })?;
    let (definition, revision) = body.into_parts();
    let (wf, problems) = state
        .engine
        .save_workflow(id, draft_of(definition)?, revision)?;
    Ok(Json(json!({"workflow": wf, "problems": problems})))
}

/// Forget a workflow. 409 while a goal or another workflow points at it —
/// the `used_by` of its row is exactly what this refuses over.
async fn remove(
    State(state): State<Shared>,
    AxPath(wfid): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_wfid(&wfid)?;
    let ws = state.engine.workspace();
    ws.get_workflow(id).map_err(|_| {
        not_found(bisa_core::text!(
            "error-node-workflows-workflow-not-found",
            id = id.to_string()
        ))
    })?;
    let usage = ws.usage_of(UsageKind::Workflow, &id.to_string())?;
    if !usage.is_empty() {
        let holders: Vec<String> = usage
            .as_slice()
            .iter()
            .map(|r| format!("{:?} {}", r.kind, r.label).to_lowercase())
            .collect();
        return Err(conflict(bisa_core::text!(
            "error-node-workflows-workflow-used-point-them-elsewhere-first",
            id = id.to_string(),
            a0 = (holders.join(", ")).to_string()
        )));
    }
    state.engine.delete_workflow(id)?;
    Ok(Json(json!({"ok": true, "workflow": id.to_string()})))
}

/// What retiring this workflow would touch.
async fn retirement(
    State(state): State<Shared>,
    AxPath(wfid): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_wfid(&wfid)?;
    state.engine.workspace().get_workflow(id).map_err(|_| {
        not_found(bisa_core::text!(
            "error-node-workflows-workflow-not-found",
            id = id.to_string()
        ))
    })?;
    let preview: RetirementDto = state.engine.preview_workflow_retirement(id)?.into();
    Ok(Json(
        json!({"workflow": id.to_string(), "retirement": preview}),
    ))
}

/// Retire the workflow on a plan. Deleting one something uses is 409 with
/// the holders — archive it instead.
async fn retire(
    State(state): State<Shared>,
    AxPath(wfid): AxPath<String>,
    crate::Body(plan): crate::Body<WorkflowPlan>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_wfid(&wfid)?;
    let ws = state.engine.workspace();
    ws.get_workflow(id).map_err(|_| {
        not_found(bisa_core::text!(
            "error-node-workflows-workflow-not-found",
            id = id.to_string()
        ))
    })?;
    if plan.workflow == Fate::Delete {
        let usage = ws.usage_of(UsageKind::Workflow, &id.to_string())?;
        if !usage.is_empty() {
            return Err(conflict(bisa_core::text!(
                "error-node-workflows-workflow-used-archive-point-them-elsewhere-first",
                id = id.to_string(),
                a0 = (usage.describe()).to_string()
            )));
        }
    }
    let done = state.engine.retire_workflow(id, plan).await?;
    Ok(Json(json!({"workflow": id.to_string(), "retired": done})))
}

/// The workflow's runs of the workspace, newest first, each numbered by its
/// place among them.
async fn runs(
    State(state): State<Shared>,
    AxPath(wfid): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_wfid(&wfid)?;
    existing(&state, id)?;
    let ws = state.engine.workspace();
    let runs = RunSummary::list(&state.engine.workflow_runs(id)?, |run| {
        crate::runs::start_detail(ws, run)
    });
    Ok(Json(json!({"workflow": id.to_string(), "runs": runs})))
}

/// Run the workflow in the workspace: `{inputs?, start?, event?}` →
/// `{run, status}`, started at once beside any other run of it — no goal is
/// captured. By hand it begins at the manual entry; naming an event start
/// makes it a test run, begun there as if `event` had happened. Refused
/// (400) when it has problems, reads the goal it would not have
/// (`needs_goal`), an input is missing or of the wrong kind, it is archived,
/// it is a goal's own design, only events begin it and none is named, or
/// the step named is no start of it.
async fn start_run(
    State(state): State<Shared>,
    AxPath(wfid): AxPath<String>,
    body: Option<crate::Body<StartRunBody>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_wfid(&wfid)?;
    let wf = existing(&state, id)?;
    let body = body.map(|crate::Body(b)| b).unwrap_or_default();
    let run = match crate::runs::way_in(Some(&wf), body.start, body.event)? {
        // The workspace listens through its switch, never through a run's
        // start: both are a run by hand.
        WayIn::Start | WayIn::ByHand => state.engine.start_workspace_run(id, body.inputs)?,
        WayIn::Test { start, payload } => {
            state
                .engine
                .test_run_workflow(id, &start, payload, body.inputs)?
        }
    };
    Ok(Json(crate::runs::made(&run)))
}

/// Stop every run of the workspace of the workflow that is going; a goal's
/// run of it is its goal's, and is left alone.
async fn stop(
    State(state): State<Shared>,
    AxPath(wfid): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_wfid(&wfid)?;
    existing(&state, id)?;
    let (runs, ended) = state.engine.stop_workflow_ended(id).await?;
    Ok(Json(
        json!({"workflow": id.to_string(), "runs": runs, "ended": ended}),
    ))
}

/// Restart every run of the workspace of the workflow that is going: each
/// cancelled, and a new run started at its start, with its inputs and its
/// event.
async fn restart(
    State(state): State<Shared>,
    AxPath(wfid): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_wfid(&wfid)?;
    existing(&state, id)?;
    let (runs, ended) = state.engine.restart_workflow_ended(id).await?;
    Ok(Json(
        json!({"workflow": id.to_string(), "runs": runs, "ended": ended}),
    ))
}

/// Put the workflow away or take it back out: `{archived}`.
async fn archive(
    State(state): State<Shared>,
    AxPath(wfid): AxPath<String>,
    crate::Body(body): crate::Body<ArchiveBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_wfid(&wfid)?;
    state.engine.workspace().get_workflow(id).map_err(|_| {
        not_found(bisa_core::text!(
            "error-node-workflows-workflow-not-found",
            id = id.to_string()
        ))
    })?;
    let wf = state.engine.archive_workflow(id, body.archived)?;
    Ok(Json(json!({"workflow": row(&state, wf)?})))
}

/// The routes this module mounts, for `bisa-node --bin api-docs` and the
/// route test. Kept beside `routes()` so a route added here is added here.
pub const ROUTES: &[RouteDoc] = &[
    RouteDoc { method: "GET", path: "/workflows", summary: "The library — installed templates and the workspace’s designs — as `{workflow, problems, workspace_problems, runs: {live, total, last?}, used_by, listening, starts, event_only, listening_needs}` rows: `workspace_problems` says why it runs on a goal only (a step reads `{goal.…}` — `needs_goal`), `runs` counts its runs of the workspace, `listening` is its standing while it is On (`null` while Off), `starts` one `{step, event, summary}` per start step, `event_only` that none of them is by hand, `listening_needs` the inputs turning it On asks; `?scope=all` includes every goal’s designs, `?goal=<id>` one goal’s, `?tag=` filters; archived workflows are left out unless `?archived=true`." },
    RouteDoc { method: "POST", path: "/workflows", summary: "Record a local workflow as a draft: `{name, description?, inputs?, steps, tags?}` → `{workflow, problems}`. Kept with its problems; a start refuses them." },
    RouteDoc { method: "POST", path: "/workflows/validate", summary: "Every problem a definition has, without recording it: `{name, steps, …}` → `{problems}`." },
    RouteDoc { method: "GET", path: "/workflows/{wfid}", summary: "One workflow, flat: `{workflow, problems, workspace_problems, runs, used_by, listening, starts, event_only, listening_needs}`." },
    RouteDoc { method: "PUT", path: "/workflows/{wfid}", summary: "Save an edited definition as its next revision: `{name, description?, inputs?, steps, tags?, revision}` → `{workflow, problems}`; 409 (`error` names both revisions) when the stored copy is no longer at `revision`." },
    RouteDoc { method: "DELETE", path: "/workflows/{wfid}", summary: "Forget a workflow and its finished runs of the workspace; 409 while a goal or another workflow uses it — archive it instead — or while one of its runs of the workspace goes (retire it). What it listened with goes with it. The bus hears `workflow_deleted`." },
    RouteDoc { method: "GET", path: "/workflows/{wfid}/retirement", summary: "What retiring the workflow would touch: `{agents, runs: [{id, status, live_steps}], history, used_by: [{kind, id, label, live}], projects_born: [{id, slug, name, adopted, workstreams, workstream_ids, sessions, archived}]}` — `runs` its runs of the workspace that are going (retired with it, whatever its fate) and `agents` their sessions, `history` how many runs of the workspace it has in all (kept under an archived workflow, deleted with a deleted one); the goal fields `harnesses`, `run`, `refusal`, `designs` are zero or null. While `used_by` is not empty, only archiving is possible; a `live` holder is a goal whose run is going, and stopping it is that goal's retirement." },
    RouteDoc { method: "POST", path: "/workflows/{wfid}/retire", summary: "Retire the workflow on a plan: `{workflow: archive|delete, projects: keep|archive|delete, tree?}` — its runs of the workspace that are going retired first (cancelled, cause `retired`, their sessions stopped and waited for), the projects its steps made settled as chosen, then the workflow archived (its runs kept as history) or deleted (its runs with it); 409 with the holders for a delete while anything uses it. → `{workflow, retired: {stopped_sessions, unsettled_sessions, projects, workstreams_retired}}`." },
    RouteDoc { method: "POST", path: "/workflows/{wfid}/archive", summary: "`{archived: true}` puts a workflow away — out of the library and the pickers, refused for a goal or a run, and turned Off: its start events are no longer heard; the goals' runs that hold a copy are untouched; 409 while one of its runs of the workspace goes (stop it, or retire the workflow) — `{archived: false}` takes it back out, Off until a person turns it On. The bus hears `workflow_archived`. → `{workflow}` (flat)." },
    RouteDoc { method: "POST", path: "/workflows/{wfid}/promote", summary: "Copy a goal’s design into the library → `{workflow}` (the copy, flat); 409 for one already there." },
    RouteDoc { method: "GET", path: "/workflows/{wfid}/runs", summary: "The workflow's runs of the workspace, newest first: `{workflow, runs: [RunSummary]}` — each with its `number` among them (1 the first), its status, who started it (`started_by`: `{by: you}`, `{by: event, event, detail?}` or `{by: test, event}`), its times, outcome and `cause`." },
    RouteDoc { method: "POST", path: "/workflows/{wfid}/runs", summary: "Run the workflow in the workspace: `{inputs?, start?, event?}` → `{run, status}` — started at once, beside any other run of it, with no goal captured; its ceiling is the workspace default (`budget.default.*`). By hand it begins at its manual entry; `start` naming an event start makes it a test run, begun there as if `event` — a sample payload, read by the start's mapping — had happened. 400 when it has problems, a step reads `{goal.…}` (`needs_goal` — it runs on a goal only), an input is missing or of the wrong kind, it is archived, it is a goal's own design (promote it first), only events begin it and no start is named, `start` names no start of it, or `event` comes with no start or with the start by hand." },
    RouteDoc { method: "POST", path: "/workflows/{wfid}/stop", summary: "Stop every run of the workspace of the workflow that is going — each cancelled (cause `stopped`), its sessions ended → `{workflow, runs}` (the runs stopped). A goal's run of the workflow is its goal's, and is left alone. Answers, summed over the runs, `ended: {sessions, terminated, still_live, children}` besides: the sessions told to stop, the harnesses that ignored it and were terminated at the deadline, the sessions that could not be ended, the spawned goals ended with it." },
    RouteDoc { method: "POST", path: "/workflows/{wfid}/restart", summary: "Restart every run of the workspace of the workflow that is going — each cancelled (cause `restarted`) and a new run started at its start, with its inputs, event and ceiling → `{workflow, runs}` (the new runs). A goal's run of the workflow is left alone. Answers, summed over the runs, `ended: {sessions, terminated, still_live, children}` besides: the sessions told to stop, the harnesses that ignored it and were terminated at the deadline, the sessions that could not be ended, the spawned goals ended with it." },
];
