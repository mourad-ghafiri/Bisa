//! Goal routes: capture, status with the current run, journal, the workflow
//! pointer, starting, queueing and amending its runs, decide (with text
//! answers and adoption inputs), close, delete, goal threads, assignees.
//!
//! A step's own verbs — answer, release, done — are the run's
//! (`runs.rs`: `/runs/{rid}/steps/{step}/…`), whichever kind of run it is;
//! a work item is addressed by its id alone (`work_items.rs`).
//!
//! A goal whose workflow begins on events **listens**: its start arms them
//! rather than running (`POST /goals/{id}/run` with no start named), and
//! what it hears, stops hearing and is called through is `listening.rs`'s.

use crate::dto::*;
use crate::route_docs::RouteDoc;
use crate::tags::{parse_tags, TagFilter};
use crate::workflows::{draft_of, resolve_workflow};
use crate::Query;
use crate::{bad_request, not_found, parse_id, ApiError, Shared};
use axum::extract::{Path as AxPath, State};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use bisa_core::tags::TagEntity;
use bisa_core::{ClosureReason, GoalId, GoalStatus, Home, RunOutcome, WayIn};
use bisa_engine::retire::{Fate, GoalPlan};
use bisa_engine::{Begun, SubmitRequest};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashSet;
use std::str::FromStr;

pub(crate) fn routes() -> Router<Shared> {
    Router::new()
        .route("/goals", get(list).post(create))
        .route("/goals/{id}", get(status).delete(remove))
        .route("/goals/{id}/journal", get(journal))
        .route("/goals/{id}/documents", get(documents).post(add_documents))
        .route("/goals/{id}/retirement", get(retirement))
        .route("/goals/{id}/retire", post(retire))
        .route("/goals/{id}/archive", post(archive))
        .route("/goals/{id}/run", get(run_detail).post(start_run))
        .route("/goals/{id}/runs", get(runs))
        .route(
            "/goals/{id}/runs/{rid}",
            get(run_by_id).delete(withdraw_run),
        )
        .route("/goals/{id}/stop", post(stop))
        .route("/goals/{id}/restart", post(restart))
        .route("/goals/{id}/workflow", put(set_workflow))
        .route("/goals/{id}/amend", post(amend))
        .route("/goals/{id}/close", post(close))
        .route("/goals/{id}/decide", post(decide))
        .route("/goals/{id}/design", post(design))
        .route(
            "/goals/{id}/messages",
            get(goal_messages).post(goal_post_message),
        )
        .route("/goals/{id}/assignees", put(set_assignees))
}

/// A goal's status is a projection of its current run, read here once per
/// goal and never stored.
pub(crate) fn status_of(ws: &bisa_store::Workspace, goal: &bisa_core::Goal) -> GoalStatus {
    goal.status(crate::runs::current_run(ws, goal).run())
}

/// The goals anything durable names — a pending gate, a question. One read
/// per request, shared by `/goals`, `/goals/{id}` and the inbox join, so
/// "waiting on you" is one fact everywhere.
pub(crate) fn owed_goals(state: &Shared) -> HashSet<GoalId> {
    state
        .engine
        .inbox()
        .into_iter()
        .filter_map(|g| g.home.goal())
        .collect()
}

/// The one row every goal listing serves. The run is loaded once and feeds
/// the status, the strip, the holder and the activity clock together.
pub(crate) fn goal_row(
    state: &Shared,
    owed: &HashSet<GoalId>,
    g: &bisa_core::Goal,
) -> Result<GoalRow, ApiError> {
    let ws = state.engine.workspace();
    let current = crate::runs::current_run(ws, g);
    let run = current.run();
    // A draft with a chosen or proposed workflow ghosts its definition.
    let definition = match (run, g.workflow) {
        (None, Some(wf)) => ws.get_workflow(wf).ok(),
        _ => None,
    };
    Ok(GoalRow {
        id: g.id.to_string(),
        title: g.title.clone(),
        statement: g.statement.clone(),
        status: g.status(run),
        workflow: g.workflow,
        run: g.run,
        run_unreadable: current.unreadable(),
        origin: g.origin.clone(),
        mode: g.mode,
        assignees: g.assignees.clone(),
        tags: g.tags.clone(),
        strip: RunStrip::from_parts(run, definition.as_ref()),
        holder: g.holder(run, owed.contains(&g.id)),
        run_status: run.map(|r| r.status()),
        queued: ws.queued_run_count(g.id)?,
        last_activity_at: last_activity_at(g, run),
        archived: g.archived,
        listening: g.listening.clone(),
    })
}

#[derive(Deserialize, Default)]
struct ListQuery {
    /// Include the goals put away — hidden unless asked.
    #[serde(default)]
    archived: bool,
}

async fn list(
    State(state): State<Shared>,
    filter: TagFilter,
    Query(q): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let ws = state.engine.workspace();
    let mut all = ws.list_goals(None)?;
    if q.archived {
        all.extend(ws.list_archived_goals()?);
    }
    let goals = filter.apply(ws, TagEntity::Goal, all, |g| g.id.to_string())?;
    let owed = owed_goals(&state);
    let mut rows = Vec::with_capacity(goals.len());
    for g in goals {
        rows.push(goal_row(&state, &owed, &g)?);
    }
    Ok(Json(json!({"goals": rows})))
}

/// THE creation path: `Engine::submit_goal_showing`. An auto or guided goal
/// without a workflow wakes the Workflow Agent — an auto goal is then adopted
/// and started by the platform, a guided one waits for the proposal's Adopt
/// gate in the inbox; a manual goal waits for its person's design. A goal
/// with a workflow and `inputs` begins its work at once: a run — or, when the
/// workflow begins on events, listening, the public hook secrets that minted
/// answered beside the goal, this once. A capture that names no mode takes
/// `goals.default_mode`.
async fn create(
    State(state): State<Shared>,
    crate::Body(body): crate::Body<NewGoalBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if body.statement.trim().is_empty() {
        return Err(bad_request(bisa_core::text!(
            "error-node-goals-statement-must-not-be-empty"
        )));
    }
    let workflow = match body.workflow.as_deref() {
        Some(raw) => Some(resolve_workflow(&state, raw)?.id),
        None => None,
    };
    let made = state.engine.submit_goal_showing(SubmitRequest {
        statement: body.statement,
        title: body.title,
        budget: Default::default(),
        mode: body
            .mode
            .unwrap_or_else(|| state.engine.default_goal_mode()),
        origin: bisa_core::GoalOrigin::Captured,
        workflow,
        start: workflow.is_some() && body.inputs.is_some(),
        inputs: body.inputs.unwrap_or_default(),
        assignees: parse_assignees(&body.assignees)?,
        tags: parse_tags(&body.tags)?,
        documents: body.documents,
    })?;
    let mut answer = json!({"goal": made.goal});
    if !made.secrets.is_empty() {
        answer["secrets"] = json!(made.secrets);
    }
    Ok(Json(answer))
}

// --- documents ------------------------------------------------------------------

/// The goal's documents — the files a person gave it as context — each with
/// its settled name, absolute path and presence on this node.
async fn documents(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_id(&id)?;
    let rows: Vec<GoalDocumentRow> = state
        .engine
        .workspace()
        .goal_documents(id)?
        .into_iter()
        .map(GoalDocumentRow::from)
        .collect();
    Ok(Json(json!({"goal": id.to_string(), "documents": rows})))
}

/// More documents for a goal: the same store writer capture uses, each
/// materialised, recorded and announced. A hash this node does not hold is
/// a bad request — the bytes go through `POST /attachments` first.
async fn add_documents(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
    crate::Body(body): crate::Body<GoalDocumentsBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_id(&id)?;
    if body.documents.is_empty() {
        return Err(bad_request(bisa_core::text!(
            "error-node-goals-documents-must-name-least-one-file"
        )));
    }
    let rows: Vec<GoalDocumentRow> = state
        .engine
        .add_goal_documents(id, &body.documents)?
        .into_iter()
        .map(GoalDocumentRow::from)
        .collect();
    Ok(Json(json!({"goal": id.to_string(), "documents": rows})))
}

fn guidance(
    state: &Shared,
    goal: &bisa_core::Goal,
    run: Option<&bisa_core::WorkflowRun>,
) -> GuidanceInfo {
    // Live gates, else the asks rebuilt from the run and the journal — the one
    // builder the inbox row uses too, so the two cannot disagree.
    let open_questions: Vec<NeedsAction> = crate::inbox::needs_actions_for(state, goal, run);
    let status = goal.status(run);
    // `design` while the Workflow Agent owes a proposal; `repair` after a
    // failed run it will be asked to fix; otherwise the status itself.
    let designs = goal.mode.designs();
    let phase = if designs && goal.workflow.is_none() && goal.run.is_none() && !goal.is_closed() {
        "design".to_string()
    } else if designs && run.is_some_and(|r| r.outcome == Some(RunOutcome::Failed)) {
        "repair".to_string()
    } else {
        status.as_str().to_string()
    };
    // The Workflow Agent's standing, from the goal's last guidance fact and
    // whether a wake is behind it right now.
    let design = state
        .engine
        .workspace()
        .journal(&Home::Goal { goal: goal.id })
        .ok()
        .and_then(|journal| {
            bisa_engine::guided::design_status(state.engine.inner(), goal, run, &journal)
        });
    GuidanceInfo {
        mode: goal.mode,
        design_enabled: state.engine.inner().config.design_enabled,
        phase,
        design,
        open_questions,
    }
}

/// Ask the Workflow Agent to design the goal's workflow again. 409 when
/// there is nothing to design or it is already at work.
async fn design(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_id(&id)?;
    state.engine.redesign(id)?;
    let ws = state.engine.workspace();
    let goal = ws.get_goal(id)?;
    let current = crate::runs::current_run(ws, &goal);
    Ok(Json(
        json!({"guidance": guidance(&state, &goal, current.run())}),
    ))
}

async fn status(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(view(&state, parse_id(&id)?)?))
}

/// The goal with everything a screen needs beside it — `GET /goals/{id}`,
/// and what the listening routes answer once the goal's standing moved.
pub(crate) fn view(state: &Shared, id: GoalId) -> Result<serde_json::Value, ApiError> {
    let ws = state.engine.workspace();
    let goal = ws.get_goal(id)?;
    let home = Home::Goal { goal: id };
    let current = crate::runs::current_run(ws, &goal);
    let run = current.run();
    let runs = RunSummary::list(&ws.list_runs(id)?, |r| crate::runs::start_detail(ws, r));
    let items = ws.list_work_items(&home)?;
    let spent = ws.spent(&home)?;
    let gates: Vec<_> = state
        .engine
        .inbox()
        .into_iter()
        .filter(|g| g.home == home)
        .collect();
    let guidance = guidance(state, &goal, run);
    let owed = owed_goals(state);
    let row = goal_row(state, &owed, &goal)?;
    // The goal's own designs, for its Workflow tab.
    let mut designs = Vec::new();
    for wf in state
        .engine
        .workspace()
        .list_workflows_in(bisa_store::WorkflowScope::Goal(id))?
    {
        designs.push(crate::workflows::row(state, wf)?);
    }
    Ok(json!({
        "goal": goal,
        "status": row.status,
        "strip": row.strip,
        "holder": row.holder,
        "last_activity_at": row.last_activity_at,
        "listening": row.listening,
        "designs": designs,
        "run": run,
        "run_unreadable": current.unreadable(),
        "runs": runs,
        "work_items": items,
        "spent": spent,
        "active": state.engine.active_item_count(&home),
        "pending_gates": gates,
        "guidance": guidance,
    }))
}

#[derive(Deserialize)]
struct JournalQuery {
    #[serde(default)]
    limit: Option<usize>,
}

async fn journal(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
    Query(q): Query<JournalQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_id(&id)?;
    let events = state.engine.workspace().journal(&Home::Goal { goal: id })?;
    let limit = q.limit.unwrap_or(100);
    let shown: Vec<_> = events.iter().rev().take(limit).rev().collect();
    Ok(Json(json!({"goal": id.to_string(), "events": shown})))
}

// --- the run ------------------------------------------------------------------

/// The goal's current run, whole: the frozen workflow and every step's record.
/// `null` when the goal has never run.
async fn run_detail(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_id(&id)?;
    let ws = state.engine.workspace();
    let goal = ws.get_goal(id)?;
    let current = crate::runs::current_run(ws, &goal);
    Ok(Json(
        json!({"run": current.run(), "run_unreadable": current.unreadable()}),
    ))
}

/// One of the goal's runs, whole, by id — the run picker's door to an
/// earlier run. A run of another goal is not this goal's and is 404, as is
/// an id that names nothing.
async fn run_by_id(
    State(state): State<Shared>,
    AxPath((id, rid)): AxPath<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_id(&id)?;
    let ws = state.engine.workspace();
    ws.get_goal(id)?;
    let rid = bisa_core::RunId::from_str(&rid).map_err(|_| {
        not_found(bisa_core::text!(
            "error-node-goals-run-not-found",
            rid = rid.to_string()
        ))
    })?;
    let run = ws.get_run(rid).map_err(|_| {
        not_found(bisa_core::text!(
            "error-node-goals-run-not-found",
            rid = rid.to_string()
        ))
    })?;
    if run.scope.goal() != Some(id) {
        return Err(not_found(bisa_core::text!(
            "error-node-goals-run-not-run-goal",
            rid = rid.to_string(),
            id = id.to_string()
        )));
    }
    Ok(Json(json!({"run": run})))
}

/// The goal's one start door: begin its work, or make a run of its
/// workflow.
///
/// With no start named it is a person's *Start*: a goal whose workflow
/// begins on events is armed and makes no run — `{status, secrets?}`, the
/// goal's status and the public hooks' secrets the start minted, shown this
/// once — and any other runs by hand. Naming the start by hand is *Run
/// now*, whatever else the workflow begins on; naming an event start is a
/// test run, begun there as if `event` had happened. A run is answered
/// `{run, status}`: started at once when nothing is live on the goal, queued
/// behind its live run otherwise — `status`, the run's, says which. Refused
/// (400) when the workflow has problems or an input is missing or of the
/// wrong kind, (409) when the goal is busy with another workflow.
async fn start_run(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
    body: Option<crate::Body<StartRunBody>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_id(&id)?;
    let body = body.map(|crate::Body(b)| b).unwrap_or_default();
    let workflow = named_start_workflow(&state, id, body.start.is_some())?;
    let run = match crate::runs::way_in(workflow.as_ref(), body.start, body.event)? {
        WayIn::Start => match state.engine.begin_goal(id, body.inputs)? {
            Begun::Run(run) => *run,
            armed @ Begun::Listening(_) => {
                let ws = state.engine.workspace();
                let armed = GoalArmed {
                    status: status_of(ws, &ws.get_goal(id)?),
                    secrets: armed.secrets(),
                };
                return Ok(Json(serde_json::to_value(armed)?));
            }
        },
        WayIn::ByHand => state.engine.start_run(id, body.inputs)?,
        WayIn::Test { start, payload } => {
            state
                .engine
                .test_run_goal(id, &start, payload, body.inputs)?
        }
    };
    Ok(Json(crate::runs::made(&run)))
}

/// The goal's workflow, read only when a run body names a start — what the
/// start is judged against. A goal with none yet answers none: the engine
/// refuses the run in its own words.
fn named_start_workflow(
    state: &Shared,
    id: GoalId,
    names_a_start: bool,
) -> Result<Option<bisa_core::Workflow>, ApiError> {
    if !names_a_start {
        return Ok(None);
    }
    let ws = state.engine.workspace();
    match ws.get_goal(id)?.workflow {
        Some(workflow) => Ok(Some(ws.get_workflow(workflow)?)),
        None => Ok(None),
    }
}

/// Every run of the goal, newest first; a queued one carries its place.
async fn runs(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_id(&id)?;
    let ws = state.engine.workspace();
    ws.get_goal(id)?;
    let runs = RunSummary::list(&ws.list_runs(id)?, |r| crate::runs::start_detail(ws, r));
    Ok(Json(json!({"goal": id.to_string(), "runs": runs})))
}

/// Take a queued run out of the goal's queue. 404 for a run of another
/// goal, 409 for one that is live or over.
async fn withdraw_run(
    State(state): State<Shared>,
    AxPath((id, rid)): AxPath<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_id(&id)?;
    state.engine.workspace().get_goal(id)?;
    let rid = bisa_core::RunId::from_str(&rid).map_err(|_| {
        not_found(bisa_core::text!(
            "error-node-goals-run-not-found",
            rid = rid.to_string()
        ))
    })?;
    let run = state.engine.withdraw_run(id, rid)?;
    Ok(Json(json!({"run": run})))
}

/// Stop the goal: its sessions ended, its queued runs withdrawn, its live
/// run cancelled. The goal stays open, ready for a new run.
async fn stop(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
    body: Option<crate::Body<StopBody>>,
) -> Result<Json<StopOutcome>, ApiError> {
    let id = parse_id(&id)?;
    let body = body.map(|crate::Body(b)| b).unwrap_or_default();
    let rationale = body.rationale.filter(|r| !r.trim().is_empty());
    let stopped = state.engine.stop_goal(id, rationale).await?;
    Ok(Json(stopped.into()))
}

/// Restart the goal: a new run of its last run's workflow and inputs,
/// started at once; a live run is cancelled first.
async fn restart(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_id(&id)?;
    let run = state.engine.restart_goal(id).await?;
    Ok(Json(crate::runs::made(&run)))
}

/// Point the goal at the workflow its next run will use.
async fn set_workflow(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
    crate::Body(body): crate::Body<SetWorkflowBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_id(&id)?;
    if let Some(definition) = body.definition {
        if body.workflow.is_some() {
            return Err(bad_request(bisa_core::text!(
                "error-node-goals-send-workflow-point-definition-record-not-both"
            )));
        }
        let (wf, problems) =
            state
                .engine
                .design_workflow(id, draft_of(definition)?, body.revision)?;
        return Ok(Json(json!({"goal": state.engine.workspace().get_goal(id)?,
                              "workflow": wf, "problems": problems})));
    }
    let workflow = match body.workflow.as_deref() {
        Some(raw) => Some(resolve_workflow(&state, raw)?.id),
        None => None,
    };
    let goal = state.engine.set_workflow(id, workflow)?;
    Ok(Json(json!({"goal": goal})))
}

/// Replace the not-yet-started steps of the current run.
async fn amend(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
    crate::Body(body): crate::Body<AmendBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_id(&id)?;
    let run = state.engine.amend_run(id, draft_of(body.workflow)?)?;
    Ok(Json(json!({"run": run})))
}

// --- closing and deciding -----------------------------------------------------

/// Close a goal. Its live run and its queued runs are cancelled, its
/// questions withdrawn, its workstreams released (the checkouts stay on
/// disk).
async fn close(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
    body: Option<crate::Body<CloseBody>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_id(&id)?;
    let body = body.map(|crate::Body(b)| b).unwrap_or(CloseBody {
        rationale: None,
        superseded_by: None,
    });
    let reason = match body.superseded_by {
        Some(by) => ClosureReason::Superseded { by: parse_id(&by)? },
        None => ClosureReason::Abandoned {
            rationale: body.rationale.filter(|r| !r.trim().is_empty()),
        },
    };
    let goal = state.engine.close_goal(id, reason)?;
    Ok(Json(json!({"goal": goal})))
}

/// Decide a gate of the goal: live path when the gate is in this engine's
/// memory, durable mirror otherwise. An adoption approved with `inputs`
/// starts the run — or, for a design that begins on events, makes the goal
/// listen with them: the outcome then carries its public hooks' secrets,
/// shown this once.
async fn decide(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
    crate::Body(body): crate::Body<DecideBody>,
) -> Result<Json<bisa_engine::DecideOutcome>, ApiError> {
    let id = parse_id(&id)?;
    let outcome = state.engine.decide_durable(
        &Home::Goal { goal: id },
        body.approve,
        body.rationale.as_deref(),
        body.answer.as_ref(),
        body.inputs,
        body.gate.as_deref(),
        body.step.as_ref(),
    )?;
    Ok(Json(outcome))
}

/// `DELETE /goals/{id}`: the plain retirement — the goal deleted, the
/// projects born of it kept (detached), its sessions stopped.
async fn remove(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_id(&id)?;
    let done = state
        .engine
        .retire_goal(
            id,
            GoalPlan {
                goal: Fate::Delete,
                projects: Fate::Keep,
                tree: false,
            },
        )
        .await?;
    Ok(Json(json!({"ok": true, "retired": done})))
}

// --- retiring ---------------------------------------------------------------

/// What retiring this goal would touch.
async fn retirement(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_id(&id)?;
    let preview: RetirementDto = state.engine.preview_goal_retirement(id)?.into();
    Ok(Json(json!({"goal": id.to_string(), "retirement": preview})))
}

/// Retire the goal on the plan: stop its work, close it, settle the projects
/// born of it, then archive or delete it.
async fn retire(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
    crate::Body(plan): crate::Body<GoalPlan>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_id(&id)?;
    let done = state.engine.retire_goal(id, plan).await?;
    Ok(Json(json!({"goal": id.to_string(), "retired": done})))
}

/// Put the goal away or take it back out: `{archived}`. Archiving is the
/// retirement with `{goal: archive, projects: keep}`; unarchiving is one move back.
async fn archive(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
    crate::Body(body): crate::Body<ArchiveBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_id(&id)?;
    if body.archived {
        state
            .engine
            .retire_goal(
                id,
                GoalPlan {
                    goal: Fate::Archive,
                    projects: Fate::Keep,
                    tree: false,
                },
            )
            .await?;
    } else {
        state.engine.unarchive_goal(id)?;
    }
    let goal = state.engine.workspace().get_goal(id)?;
    Ok(Json(json!({"goal": goal})))
}

// --- goal thread ----------------------------------------------------------

async fn goal_messages(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
    Query(q): Query<crate::conversation::PageQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_id(&id)?; // validates
    crate::conversation::scope_messages(&state, &id.to_string(), &q)
}

async fn goal_post_message(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
    crate::Body(body): crate::Body<NewMessageBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_id(&id)?;
    crate::conversation::scope_post(&state, &id.to_string(), body)
}

/// Parse the wire form (`agent:<id>` / `human:<hex>` / `team:<id>`) into
/// assignees, refusing the whole list on the first bad entry.
pub(crate) fn parse_assignees(raw: &[String]) -> Result<Vec<bisa_core::Assignee>, ApiError> {
    raw.iter()
        .map(|s| {
            s.parse::<bisa_core::Assignee>().map_err(|e| {
                bad_request(bisa_core::text!(
                    "error-node-goals-refused",
                    detail = e.to_string()
                ))
            })
        })
        .collect()
}

/// Set who carries this goal: its agents take the work, its humans may
/// decide its gates. `{"assignees": []}` clears it.
async fn set_assignees(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
    crate::Body(body): crate::Body<AssigneesBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_id(&id)?;
    let assignees = parse_assignees(&body.assignees)?;
    let goal = bisa_engine::ops::set_goal_assignees(state.engine.inner(), id, assignees)?;
    Ok(Json(json!({"goal": goal})))
}

/// The routes this module mounts, for `bisa-node --bin api-docs` and the
/// route test. Kept beside `routes()` so a route added here is added here.
pub const ROUTES: &[RouteDoc] = &[
    RouteDoc { method: "GET", path: "/goals", summary: "Every goal as a row: id, title, statement, status, workflow, run, origin, mode, assignees, tags — plus its `strip` (every step with its state), the `holder` (you · agents · world · finished · design), `run_status` (the current run's), `queued` (how many runs wait behind the live one), `last_activity_at`, `archived` and `listening` (its standing while its workflow begins on events — armed, or paused and why; `null` otherwise). `run_unreadable` is true when the goal names a run this node cannot read: the row is then drawn as for a goal with no run, never dropped and never the reason the list fails. Archived goals are left out unless `?archived=true`." },
    RouteDoc { method: "POST", path: "/goals", summary: "Capture a goal: `{statement, title?, mode?, workflow?, inputs?, assignees?, tags?, documents?}` → `{goal, secrets?}`. `mode` is `auto` (the Workflow Agent designs and the platform adopts, starts, repairs and restarts alone), `guided` (it proposes, you adopt) or `manual` (you design on the Workflow tab); absent takes `goals.default_mode`. An auto or guided goal without a workflow wakes the Workflow Agent; `inputs` with a workflow begins its work at once — a run, or, when the workflow begins on events, listening with those inputs, `secrets` then the public hooks' secrets that minted (`{step, path, secret}`, **shown once**, absent when there are none); `documents` — descriptors from `POST /attachments` — are kept under the goal's `documents/` folder as its context before anything runs (400 for a hash this node does not hold)." },
    RouteDoc { method: "GET", path: "/goals/{id}/documents", summary: "The files a person gave the goal as context: `{goal, documents: [{file, name, path, present}]}` — the descriptor, the name it is kept under in `documents/`, its absolute path, and whether the bytes are on this node." },
    RouteDoc { method: "POST", path: "/goals/{id}/documents", summary: "Give the goal more documents: `{documents: [AttachmentRef]}` → the rows added. Each is materialised under `documents/` (a taken name numbered), recorded as a `document` journal fact and announced as `document.added`; 400 for an empty list or a hash this node does not hold." },
    RouteDoc { method: "GET", path: "/goals/{id}", summary: "The goal with its status, strip, holder, `listening`, its own designs, current run (`run_unreadable` when it names one this node cannot read), its runs newest first (a queued one carries its `position`, a cancelled one its `cause`), work items, spend, pending gates and guidance." },
    RouteDoc { method: "DELETE", path: "/goals/{id}", summary: "Delete a goal — the plain retirement: refused up front while a design of its own is used elsewhere; every session on it aborted and waited for, its unfinished run cancelled, its journal, runs, work items, designs and attachments gone; the projects born of it kept and detached. → `{ok, retired}`." },
    RouteDoc { method: "GET", path: "/goals/{id}/retirement", summary: "What retiring the goal would touch: `{agents, harnesses, run: {id, status, live_steps} | null, refusal: string | null, designs, projects_born: [{id, slug, name, adopted, workstreams, workstream_ids, sessions, archived}], projects_attached: […]}` — the facts a retirement dialog is drawn from: the engine sessions it aborts, the terminal harnesses it terminates, the run it cancels, and why a deletion would be refused (archive stays open)." },
    RouteDoc { method: "POST", path: "/goals/{id}/retire", summary: "Retire the goal on a plan: `{goal: archive|delete, projects: keep|archive|delete, tree?}`. Refuses first (a design of its own used elsewhere, for a delete), then stops every session on it and in the projects the plan touches — the harness process aborted on the spot — closes it if open, waits up to 5 s for the rows to end, settles the projects born of it (`tree` moves a deleted managed folder to the Trash; an adopted folder never moves), then archives or deletes it; a merely attached project is detached on delete and left alone on archive. `goal_deleted` is scoped to the goal. → `{goal, retired: {stopped_sessions, unsettled_sessions, projects: [{id, fate}], workstreams_retired}}`." },
    RouteDoc { method: "POST", path: "/goals/{id}/archive", summary: "`{archived: true}` puts a goal away (closing it first when open; a mark, never a status), `{archived: false}` takes it back out — it stays closed. → `{goal}`." },
    RouteDoc { method: "GET", path: "/goals/{id}/journal", summary: "The signed journal, rendered, newest last (`?limit=`, default 100)." },
    RouteDoc { method: "GET", path: "/goals/{id}/run", summary: "The current run, whole — the live one, else the latest that started: the frozen workflow and every step's record; `null` before the first run. Never a queued run. Its steps are answered, released and marked done by the run (`/runs/{rid}/steps/{step}/…`)." },
    RouteDoc { method: "POST", path: "/goals/{id}/run", summary: "The goal's one start door: `{inputs?, start?, event?}`. With no `start` it is a person's start — a goal whose workflow begins on events is armed and makes no run → `{status, secrets?}` (the goal's status; the public hooks' secrets the start minted, `{step, path, secret}`, **shown once**): it listens with `inputs`, or, given none, with what it listened with before — a goal started again after a pause is asked for nothing it was already given; any other goal runs by hand → `{run, status}` (the run's status). `start` naming the start by hand is a run now, whatever else the workflow begins on; naming an event start makes a test run, begun there as if `event` — a sample payload — had happened. A run is started at once when nothing is live on the goal (`status` running or waiting), queued behind its live run otherwise (`queued`): it starts on its own, in order, when the live run ends. 400 for a `start` that is no start of the workflow, or an `event` with no start or with the start by hand; 409 when the goal is busy and points at another workflow." },
    RouteDoc { method: "GET", path: "/goals/{id}/runs", summary: "Every run of the goal, newest first: `{goal, runs: [{id, scope, goal, number, status, workflow, workflow_name, revision, started_by, queued_at, started_at?, finished_at?, outcome?, cause?, position?}]}` — `number` its place among the goal's runs (1 the first), a queued run's `position` its place in the queue (1 next), a cancelled run's `cause` `stopped` · `restarted` · `withdrawn` · `closed`, `started_by` who started it — `{by: you}`, `{by: event, event, detail?}` (the event's kind, and who sent the message, which signal or which run when there is one to name) or `{by: test, event}`." },
    RouteDoc { method: "GET", path: "/goals/{id}/runs/{rid}", summary: "One of the goal's runs, whole, by id — an earlier or a queued run for the run picker. 404 for a run of another goal." },
    RouteDoc { method: "DELETE", path: "/goals/{id}/runs/{rid}", summary: "Withdraw a queued run from the goal's queue → `{run}` (cancelled, cause `withdrawn`). 404 for a run of another goal, 409 for one that is live or over." },
    RouteDoc { method: "POST", path: "/goals/{id}/stop", summary: "Stop the goal: `{rationale?}` → `{stopped, withdrawn}` — its sessions ended, its queued runs withdrawn, its live run cancelled (cause `stopped`), its listening stopped. The goal stays open and reads `draft`, ready for a new run; nothing to stop answers both empty. 409 when closed." },
    RouteDoc { method: "POST", path: "/goals/{id}/restart", summary: "Restart the goal → `{run, status}`: a new run of its last run's workflow and inputs, at the same start and on the same event, started at once ahead of the queue; a live run is cancelled first (cause `restarted`). 409 when closed or never run." },
    RouteDoc { method: "PUT", path: "/goals/{id}/workflow", summary: "Point the goal at a workflow — `{workflow: <id | slug> | null}` → `{goal}` — or record `{definition: {name, steps, ...}, revision?}` as the goal’s own design, kept as a draft → `{goal, workflow, problems}` (`revision` names the design edited once the goal has one; 409 when it moved). Refused while a run is live or queued." },
    RouteDoc { method: "POST", path: "/goals/{id}/amend", summary: "Replace the not-yet-started steps of the current run: `{workflow: {name, steps, …}}` → `{run}`." },
    RouteDoc { method: "POST", path: "/goals/{id}/close", summary: "Close a goal: `{rationale?, superseded_by?}`. Cancels its live run and its queued runs (cause `closed`), withdraws its questions, releases workstreams." },
    RouteDoc { method: "POST", path: "/goals/{id}/decide", summary: "Decide a gate: `{approve, rationale?, answer?, gate?, inputs?, step?}` → `DecideOutcome` (`home`, `gate`, `approve`, `status`, `secrets?`). An approved adoption starts the run with `inputs` — or, for a design that begins on events, makes the goal listen with them, and `secrets` carries its public hooks' secrets, shown once; an approved amendment applies it; `step` names the waiting step when several wait — and a held `wait` step is released only by name while an adoption or an amendment is still owed a decision. A gate is decided once: a second decision is `409`; a `gate` that is another home's is `400` and decides nothing; an answer the gate does not offer leaves it open. A waiting step is also decided by its run (`POST /runs/{rid}/decide`)." },
    RouteDoc { method: "POST", path: "/goals/{id}/design", summary: "Ask the Workflow Agent to design the goal’s workflow again — after a stall, a failure or a restart → `{guidance}`. 409 when the goal is manual, is closed, already has a workflow or a run, is being worked on, or designing is off on this node." },
    RouteDoc { method: "GET", path: "/goals/{id}/messages", summary: "The goal's conversation (`?before=&before_id=&limit=`)." },
    RouteDoc { method: "POST", path: "/goals/{id}/messages", summary: "Post into the goal's conversation: `{content, reply_to?, mentions?, attachments?, artifacts?}`." },
    RouteDoc { method: "PUT", path: "/goals/{id}/assignees", summary: "Set who carries the goal: `{assignees: [\"agent:…\", \"human:…\", \"team:…\"]}`." },
];
