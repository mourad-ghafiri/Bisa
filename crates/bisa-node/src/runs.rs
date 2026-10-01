//! Runs: one run by its id — a goal's or the workspace's — and what a person
//! does to it, plus the one reader for a goal's current run.
//!
//! **The run routes** address a run by its id alone, whichever kind it is:
//! read it (`GET /runs/{rid}`), answer, release or mark done one of its
//! steps, decide what it owes (through its home — the goal, or the run
//! itself). Stop and restart are a run of the workspace's alone: a goal's
//! run is stopped and restarted from its goal, whose queue it belongs to, and
//! the engine says so (409).
//!
//! **The current-run reader.** A goal names its run; the run file may be
//! gone or written by another shape of the code (`WorkflowRun` refuses an
//! unknown field). Two policies used to answer that — the inbox swallowed
//! the error, the goals list failed whole with it — so the same workspace
//! read one way on one screen and 500 on the next. Now every route reads
//! through [`current_run`]: an unreadable run is said once at `error`,
//! naming the goal and the run, and the row carries
//! [`CurrentRun::Unreadable`] so the screen can say why the goal reads as if
//! it had no run instead of lying about it.
//!
//! **How a run is asked for.** `POST /goals/{id}/run` and
//! `POST /workflows/{wfid}/runs` take one body, and [`way_in`] reads which of
//! the engine's doors it names — the core's reading ([`WayIn`]), the one the
//! command line makes of its flags: a person's start, a run now at the start
//! by hand, or a test run of an event start with a sample of its event.

use crate::dto::*;
use crate::route_docs::RouteDoc;
use crate::Query;
use crate::{bad_request, not_found, ApiError, Shared};
use axum::extract::{Path as AxPath, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use bisa_core::{
    Goal, NoWayIn, RunId, RunScope, SignalSource, StepId, WayIn, Workflow, WorkflowRun,
};
use bisa_store::Workspace;
use serde_json::json;
use std::str::FromStr;

pub(crate) fn routes() -> Router<Shared> {
    Router::new()
        .route("/runs/{rid}", get(detail))
        .route("/runs/{rid}/journal", get(journal))
        .route("/runs/{rid}/stop", post(stop))
        .route("/runs/{rid}/restart", post(restart))
        .route("/runs/{rid}/decide", post(decide))
        .route("/runs/{rid}/steps/{step}/answer", post(answer_step))
        .route("/runs/{rid}/steps/{step}/release", post(release_step))
        .route("/runs/{rid}/steps/{step}/done", post(mark_step_done))
}

/// A goal's current run as a read route sees it.
pub(crate) enum CurrentRun {
    /// The goal has never run.
    None,
    /// Boxed: a run carries its whole workflow, and the other two arms are
    /// a word each.
    Run(Box<WorkflowRun>),
    /// The goal names a run this node cannot read — missing, or another
    /// build's shape. Drawn as a goal with no run, and said so.
    Unreadable,
}

impl CurrentRun {
    pub(crate) fn run(&self) -> Option<&WorkflowRun> {
        match self {
            CurrentRun::Run(run) => Some(run),
            CurrentRun::None | CurrentRun::Unreadable => None,
        }
    }

    pub(crate) fn unreadable(&self) -> bool {
        matches!(self, CurrentRun::Unreadable)
    }

    pub(crate) fn into_run(self) -> Option<WorkflowRun> {
        match self {
            CurrentRun::Run(run) => Some(*run),
            CurrentRun::None | CurrentRun::Unreadable => None,
        }
    }
}

/// The goal's current run, read once per row; an unreadable one is an
/// `error` line here and `Unreadable` to the caller, never a failed list.
pub(crate) fn current_run(ws: &Workspace, goal: &Goal) -> CurrentRun {
    let Some(id) = goal.run else {
        return CurrentRun::None;
    };
    match ws.get_run(id) {
        Ok(run) => CurrentRun::Run(Box::new(run)),
        Err(e) => {
            tracing::error!(
                target: "bisa_node",
                goal = %goal.id,
                run = %id,
                "the goal's current run cannot be read; drawing the goal without it: {e}"
            );
            CurrentRun::Unreadable
        }
    }
}

/// The run a route names — any run, of a goal or of the workspace — or its
/// 404: an id that is no run's id, or names no run. A run that is there and
/// cannot be read is the store's own error, never *not found*: a file this
/// node cannot read is said, not hidden behind a run that does not exist.
fn run_of(state: &Shared, raw: &str) -> Result<WorkflowRun, ApiError> {
    let missing = || {
        not_found(bisa_core::text!(
            "error-node-goals-run-not-found",
            rid = raw.to_string()
        ))
    };
    let id = RunId::from_str(raw).map_err(|_| missing())?;
    match state.engine.workspace().get_run(id) {
        Ok(run) => Ok(run),
        Err(bisa_store::StoreError::RunNotFound(_)) => Err(missing()),
        Err(e) => Err(e.into()),
    }
}

pub(crate) fn parse_step(s: &str) -> Result<StepId, ApiError> {
    StepId::new(s).map_err(|e| {
        bad_request(bisa_core::text!(
            "error-node-goals-refused",
            detail = e.to_string()
        ))
    })
}

/// Which of the engine's doors a run body names (`StartRunBody`): what
/// `start` and `event` ask of `workflow` — absent when the home has none
/// yet, which the engine refuses in its own words. The reading is the
/// core's ([`WayIn::of`]); the start a person runs by hand reads no event,
/// and an event needs the start that reads it — both are refused here, in
/// the words of a body.
pub(crate) fn way_in(
    workflow: Option<&Workflow>,
    start: Option<StepId>,
    event: Option<serde_json::Value>,
) -> Result<WayIn, ApiError> {
    WayIn::of(workflow, start, event).map_err(|refused| {
        bad_request(match refused {
            NoWayIn::EventWithoutStart => {
                bisa_core::text!("error-node-runs-event-needs-its-start")
            }
            NoWayIn::ByHandReadsNoEvent { step } => bisa_core::text!(
                "error-node-runs-start-by-hand-reads-no-event",
                step = step.to_string()
            ),
        })
    })
}

/// A run just made, as the two run routes answer it: `{run, status}`.
pub(crate) fn made(run: &WorkflowRun) -> serde_json::Value {
    json!({"run": run, "status": run.status()})
}

/// Its place among its siblings — the goal's runs, or its workflow's runs
/// of the workspace — oldest first, 1 the first.
fn number_of(ws: &Workspace, run: &WorkflowRun) -> Result<usize, ApiError> {
    let siblings = match &run.scope {
        RunScope::Goal { goal } => ws.list_runs(*goal)?,
        RunScope::Workspace { .. } => ws.list_workflow_runs(run.workflow.id)?,
    };
    Ok(siblings
        .iter()
        .position(|r| r.id == run.id)
        .map_or(siblings.len() + 1, |i| i + 1))
}

/// What there is to say about the event that began a run, beyond its kind
/// (`StartedBy`): the signal's name, the platform's topic, who wrote the
/// message, which run ended. Nothing for a run no event began, and nothing
/// for the kinds whose word says it all. A name that no longer resolves —
/// an agent removed, a run deleted — is left unsaid, never guessed. Public:
/// the command line says who started a run in the same words when it reads
/// the workspace itself.
pub fn start_detail(ws: &Workspace, run: &WorkflowRun) -> Option<String> {
    let event = run.event.as_ref()?;
    let said = |key: &str| event.payload.get(key).and_then(serde_json::Value::as_str);
    match event.source {
        SignalSource::Signal | SignalSource::Platform => event.name.clone(),
        SignalSource::Message => author_name(ws, said("author")?, said("author_kind")?),
        SignalSource::Run => run_title(ws, said("run")?),
        SignalSource::Schedule
        | SignalSource::Hook
        | SignalSource::Project
        | SignalSource::Connector
        | SignalSource::Check
        | SignalSource::Test => None,
    }
}

/// Who wrote a message, by name: an agent's, or the label a person goes by
/// here. The event says which (`author_kind`: `agent`, else a person — you
/// among them).
fn author_name(ws: &Workspace, author: &str, kind: &str) -> Option<String> {
    if kind == "agent" {
        let id = bisa_core::AgentId::new(author).ok()?;
        return ws.get_agent(&id).ok().map(|agent| agent.name);
    }
    let pubkey = bisa_core::PrincipalId::new(author).ok()?;
    ws.member(&pubkey).ok().flatten()?.label
}

/// A run by its workflow's name and its number — *Nightly report #4*.
fn run_title(ws: &Workspace, raw: &str) -> Option<String> {
    let run = ws.get_run(raw.parse().ok()?).ok()?;
    let number = number_of(ws, &run).ok()?;
    Some(format!("{} #{number}", run.workflow.name))
}

/// What the run owes a person: a run of the workspace's own asks; for a
/// goal's run, the goal's asks that are this run's — an adoption or a
/// question about the goal is the goal's, never the run's.
fn needs_of(state: &Shared, run: &WorkflowRun) -> Result<Vec<NeedsAction>, ApiError> {
    match &run.scope {
        RunScope::Workspace { .. } => Ok(crate::inbox::needs_actions_for_run(state, run)),
        RunScope::Goal { goal } => {
            let ws = state.engine.workspace();
            let goal = ws.get_goal(*goal)?;
            let current = current_run(ws, &goal);
            let id = run.id.to_string();
            Ok(crate::inbox::needs_actions_for(state, &goal, current.run())
                .into_iter()
                .filter(|a| a.run.as_deref() == Some(id.as_str()))
                .collect())
        }
    }
}

/// One run, whole: `RunView`.
async fn detail(
    State(state): State<Shared>,
    AxPath(rid): AxPath<String>,
) -> Result<Json<RunView>, ApiError> {
    let run = run_of(&state, &rid)?;
    let ws = state.engine.workspace();
    let number = number_of(ws, &run)?;
    let needs_actions = needs_of(&state, &run)?;
    Ok(Json(RunView {
        summary: RunSummary::of(&run, number, start_detail(ws, &run)),
        holder: run.holder(!needs_actions.is_empty()),
        needs_actions,
        run,
    }))
}

#[derive(serde::Deserialize)]
struct JournalQuery {
    #[serde(default)]
    limit: Option<usize>,
}

/// The journal of the run's home, newest last: a run of the workspace's own,
/// or its goal's for a goal's run (which files every run of the goal).
async fn journal(
    State(state): State<Shared>,
    AxPath(rid): AxPath<String>,
    Query(q): Query<JournalQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let run = run_of(&state, &rid)?;
    let home = run.home();
    let events = state.engine.workspace().journal(&home)?;
    let limit = q.limit.unwrap_or(100);
    let shown: Vec<_> = events.iter().rev().take(limit).rev().collect();
    Ok(Json(
        json!({"run": run.id.to_string(), "home": home, "events": shown}),
    ))
}

/// Stop a run of the workspace: `{rationale?}` → `{run}`.
async fn stop(
    State(state): State<Shared>,
    AxPath(rid): AxPath<String>,
    body: Option<crate::Body<StopBody>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let run = run_of(&state, &rid)?;
    let body = body.map(|crate::Body(b)| b).unwrap_or_default();
    let rationale = body.rationale.filter(|r| !r.trim().is_empty());
    let run = state.engine.stop_run(run.id, rationale).await?;
    Ok(Json(json!({"run": run})))
}

/// Restart a run of the workspace → `{run, status}`: the new run.
async fn restart(
    State(state): State<Shared>,
    AxPath(rid): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let run = run_of(&state, &rid)?;
    let run = state.engine.restart_run(run.id).await?;
    Ok(Json(made(&run)))
}

/// Decide what the run owes, through its home — the live gate when this
/// engine holds it, the durable mirror otherwise.
async fn decide(
    State(state): State<Shared>,
    AxPath(rid): AxPath<String>,
    crate::Body(body): crate::Body<DecideBody>,
) -> Result<Json<bisa_engine::DecideOutcome>, ApiError> {
    let run = run_of(&state, &rid)?;
    let outcome = state.engine.decide_durable(
        &run.home(),
        body.approve,
        body.rationale.as_deref(),
        body.answer.as_ref(),
        body.inputs,
        body.gate.as_deref(),
        body.step.as_ref(),
    )?;
    Ok(Json(outcome))
}

async fn answer_step(
    State(state): State<Shared>,
    AxPath((rid, step)): AxPath<(String, String)>,
    crate::Body(body): crate::Body<StepAnswerBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let run = run_of(&state, &rid)?;
    let run = state
        .engine
        .answer_step(run.id, &parse_step(&step)?, &body.answer)?;
    Ok(Json(json!({"run": run})))
}

/// Let a held `wait` step go: `{payload?}` → `{run}`. A payload is the
/// step's output, as what a wait hears is.
async fn release_step(
    State(state): State<Shared>,
    AxPath((rid, step)): AxPath<(String, String)>,
    body: Option<crate::Body<ReleaseStepBody>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let run = run_of(&state, &rid)?;
    let body = body.map(|crate::Body(b)| b).unwrap_or_default();
    let run = state
        .engine
        .release_step(run.id, &parse_step(&step)?, body.payload)?;
    Ok(Json(json!({"run": run})))
}

async fn mark_step_done(
    State(state): State<Shared>,
    AxPath((rid, step)): AxPath<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let run = run_of(&state, &rid)?;
    let run = state.engine.mark_step_done(run.id, &parse_step(&step)?)?;
    Ok(Json(json!({"run": run})))
}

/// The routes this module mounts, for `bisa-node --bin api-docs` and the
/// route test. Kept beside `routes()` so a route added here is added here.
pub const ROUTES: &[RouteDoc] = &[
    RouteDoc { method: "GET", path: "/runs/{rid}", summary: "One run — a goal's or the workspace's — whole: `{run, summary, holder, needs_actions}` — the frozen workflow and every step's record, its `RunSummary` (`scope`, `goal`, `number` among its goal's runs or its workflow's runs of the workspace, `started_by` — `{by: you}`, `{by: event, event, detail?}` or `{by: test, event}` — times, outcome, cause), who it waits on, and what it owes a person, each ask with the `home` it is decided through. 404 for an id that names no run." },
    RouteDoc { method: "GET", path: "/runs/{rid}/journal", summary: "The journal of the run's home, rendered, newest last (`?limit=`, default 100): `{run, home, events}` — a run of the workspace's own facts, or its goal's journal for a goal's run." },
    RouteDoc { method: "POST", path: "/runs/{rid}/stop", summary: "Stop a run of the workspace: `{rationale?}` → `{run}` — cancelled (cause `stopped`), its sessions ended; a run already over is answered as it is. 409 for a goal's run: stop it from its goal." },
    RouteDoc { method: "POST", path: "/runs/{rid}/restart", summary: "Restart a run of the workspace → `{run, status}` (the new run): a live one is cancelled first (cause `restarted`), then its workflow runs again at the same start, with the same inputs, event and ceiling. 409 for a goal's run: restart it from its goal." },
    RouteDoc { method: "POST", path: "/runs/{rid}/decide", summary: "Decide what the run owes, through its home — a run of the workspace's own gate or question, or its goal's for a goal's run: `{approve, rationale?, answer?, gate?, step?}` → `DecideOutcome` (`home`, `gate`, `approve`, `status` — `{of: goal|run, status}` — and `secrets?`, the public hook secrets an adoption minted when it made its goal listen, shown once). `step` names the waiting step when several wait; a gate is decided once (a second decision is `409`), another home's gate is `400`." },
    RouteDoc { method: "POST", path: "/runs/{rid}/steps/{step}/answer", summary: "Answer a waiting `human` step of the run — of either kind: `{answer}` → `{run}`." },
    RouteDoc { method: "POST", path: "/runs/{rid}/steps/{step}/release", summary: "Release a `wait` step of the run a person is holding: `{payload?}` → `{run}` — a payload, when given, is the step's output." },
    RouteDoc { method: "POST", path: "/runs/{rid}/steps/{step}/done", summary: "Mark a `human` step of the run done by hand → `{run}`." },
];
