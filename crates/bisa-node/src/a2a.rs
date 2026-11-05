//! A2A (Agent2Agent) expose side — feature `a2a`.
//!
//! Implements the A2A **v0.3.0** wire (JSON-RPC 2.0 over HTTP): the agent
//! card at `/.well-known/agent-card.json` and a `POST /a2a` endpoint with
//! `message/send`, `tasks/get`, `tasks/cancel`. Streaming and push
//! notifications are not offered (`capabilities` says so honestly).
//!
//! Mapping: an A2A Task **is** a goal (`id` and `contextId` are the goal's
//! id). The task's state is the goal's status: `draft → submitted` (or
//! `input-required` while an adoption or a start is owed), `running →
//! working`, `waiting → input-required`, `done → completed` (the run's step
//! outputs attached as artifacts), `failed → failed`, `closed → canceled`.
//!
//! What runs is [`A2aExposeConfig::workflow`]: with one configured, every
//! task is a goal on that workflow, started at once when `auto_start` is set
//! and otherwise waiting for a person; without one, the task is an auto goal
//! the Workflow Agent designs a workflow for, and the platform runs.
//!
//! **The door's own checks.** The card and the endpoint answer without the
//! control-plane token (`auth::is_exempt`). The endpoint refuses a browser's
//! call by its `Origin` header (a page on any origin can reach a loopback
//! port; an agent sends none) and anything but `application/json`, takes an
//! optional bearer of its own ([`A2aExposeConfig::token`], advertised on the
//! card under `securitySchemes`), reaches only the goals it made (tagged
//! [`A2A_TAG`]), and starts nothing from a caller's words until the content
//! screen passed them or a person let the held signal through — the task
//! reads `input-required` or `submitted` meanwhile. Exposure beyond loopback
//! is the operator's tunnel or reverse proxy, and a door exposed with no
//! bearer is said once in the log at mount.

use crate::route_docs::RouteDoc;
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use bisa_core::{ClosureReason, Goal, GoalId, GoalStatus, Tags, WorkflowId, WorkflowRun};
use bisa_engine::{OutsideGoal, SubmitRequest};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::str::FromStr;

use crate::Shared;

/// A2A protocol revision this module implements.
pub const A2A_PROTOCOL_VERSION: &str = "0.3.0";

/// The named signal an inbound A2A task raises, on the goal it made. A
/// workflow that begins on it — `on = { event = "signal", name = "a2a.task" }`
/// — or waits for it reacts to a remote agent's request. What the task says
/// came from outside: the signal is redacted and read by the content screen
/// before anything hears it.
pub const A2A_TASK_SIGNAL: &str = "a2a.task";

/// Where an A2A task's signal came from, in the words the content screen and
/// a person read.
const A2A_SOURCE: &str = "an A2A task";

/// The tag every goal A2A makes carries, and the one `tasks/get`,
/// `tasks/cancel` and a continuation reach: a caller sees its own tasks and
/// no other goal of the workspace. A person who takes the tag off hides the
/// task from its caller; one who puts it on a goal opens it — both their
/// deliberate acts.
pub const A2A_TAG: &str = "a2a";

/// JSON-RPC error codes (A2A §8.2).
const TASK_NOT_FOUND: i64 = -32001;
const TASK_NOT_CANCELABLE: i64 = -32002;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct A2aSkill {
    pub id: String,
    pub name: String,
    pub description: String,
}

/// Configuration for exposing this node as an A2A agent — `<data>/a2a.toml`,
/// a file a person writes: a key it does not know is refused by name, never
/// dropped (`workflow` misspelt would make every task an auto goal).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct A2aExposeConfig {
    /// Public base URL callers reach this node at (e.g. behind a tunnel);
    /// the JSON-RPC endpoint is `<public_base_url>/a2a`.
    pub public_base_url: String,
    /// Advertised skills; empty = the default "run-goal" skill.
    #[serde(default)]
    pub skills: Vec<A2aSkill>,
    /// The workflow every inbound task runs. `None` makes each task an auto
    /// goal: the Workflow Agent designs, the platform adopts and starts.
    #[serde(default)]
    pub workflow: Option<WorkflowId>,
    /// With `workflow`, start the run the moment the task arrives — once the
    /// content screen passed its words. Default false — the task sits
    /// `input-required` until a person starts it.
    #[serde(default)]
    pub auto_start: bool,
    /// A bearer of the door's own: with one, `POST /a2a` takes
    /// `Authorization: Bearer <token>` and the card advertises the scheme;
    /// without one the endpoint is open to whoever reaches it, and the node
    /// says so once at mount.
    #[serde(default)]
    pub token: Option<String>,
}

impl A2aExposeConfig {
    pub fn new(public_base_url: impl Into<String>) -> Self {
        Self {
            public_base_url: public_base_url.into(),
            skills: Vec::new(),
            workflow: None,
            auto_start: false,
            token: None,
        }
    }
}

/// Routes mounted onto the node router when the feature + config are on.
pub(crate) fn routes() -> Router<Shared> {
    Router::new()
        .route("/.well-known/agent-card.json", get(agent_card))
        .route("/a2a", post(rpc))
}

// ---------------------------------------------------------------------------
// Agent card
// ---------------------------------------------------------------------------

async fn agent_card(State(state): State<Shared>) -> Json<Value> {
    let Some(cfg) = state.a2a.as_ref() else {
        // For the calling system, never a person: English, as the protocol reads it.
        return Json(json!({"error": "a2a not configured"})); // LCOV_EXCL_LINE: the routes are mounted with a configuration; by construction
    };
    let skills: Vec<Value> = if cfg.skills.is_empty() {
        vec![json!({
            "id": "run-goal",
            "name": "Run a goal",
            "description": "Take a stated outcome through a workflow to a finished run",
            "tags": ["workflows"],
        })]
    } else {
        cfg.skills
            .iter()
            .map(|s| json!({"id": s.id, "name": s.name, "description": s.description, "tags": []}))
            .collect()
    };
    let mut card = json!({
        "protocolVersion": A2A_PROTOCOL_VERSION,
        "name": "bisa",
        "description": "Bisa node: runs goals through workflows of agent, human, check and wait steps, with human approval gates.",
        "url": format!("{}/a2a", cfg.public_base_url.trim_end_matches('/')),
        "preferredTransport": "JSONRPC",
        "version": env!("CARGO_PKG_VERSION"),
        "provider": {"organization": "Bisa", "url": env!("CARGO_PKG_HOMEPAGE")},
        "documentationUrl": env!("CARGO_PKG_HOMEPAGE"),
        "capabilities": {
            "streaming": false,
            "pushNotifications": false,
            "stateTransitionHistory": false,
        },
        "defaultInputModes": ["text/plain", "application/json"],
        "defaultOutputModes": ["text/plain", "application/json"],
        "skills": skills,
    });
    // The door's bearer, when it has one: the scheme and that every call
    // needs it (A2A v0.3.0 §4). A card without one promises nothing.
    if cfg.token.is_some() {
        card["securitySchemes"] = json!({"bearer": {"type": "http", "scheme": "bearer"}});
        card["security"] = json!([{"bearer": []}]);
    }
    Json(card)
}

/// The refusal the endpoint answers before a byte of the body is read, if
/// any — each an HTTP answer and never a JSON-RPC one (A2A v0.3.0 §4: a transport refusal is
/// the transport's): a call carrying an `Origin` header — a browser's, on
/// whatever page found the loopback port; an agent sends none — is 403; a
/// content type whose essence is not `application/json` is 415; with a
/// bearer configured, a missing or wrong `Authorization` is 401 with the
/// challenge. The bodies are the machine's, in English, as the hook door's.
pub(crate) fn admit(headers: &HeaderMap, cfg: &A2aExposeConfig) -> Option<Response> {
    if headers.contains_key(header::ORIGIN) {
        return Some(
            (
                StatusCode::FORBIDDEN,
                Json(json!({"error": "forbidden: a browser origin"})),
            )
                .into_response(),
        );
    }
    let essence = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|v| {
            v.split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase()
        });
    if essence.as_deref() != Some("application/json") {
        return Some(
            (
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                Json(json!({"error": "unsupported media type: application/json is required"})),
            )
                .into_response(),
        );
    }
    if let Some(expected) = cfg.token.as_deref() {
        let given = crate::auth::presented_in_header(headers);
        if !given.is_some_and(|given| crate::auth::same(&given, expected)) {
            let mut response = (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "unauthorized"})),
            )
                .into_response();
            response.headers_mut().insert(
                header::WWW_AUTHENTICATE,
                "Bearer".parse().expect("a header value"),
            );
            return Some(response);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// JSON-RPC endpoint
// ---------------------------------------------------------------------------

fn rpc_ok(id: Value, result: Value) -> Json<Value> {
    Json(json!({"jsonrpc": "2.0", "id": id, "result": result}))
}

fn rpc_err(id: Value, code: i64, message: &str) -> Json<Value> {
    Json(json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}))
}

/// The whole endpoint takes the raw body so malformed JSON gets a proper
/// JSON-RPC `-32700` instead of an axum 400 — once [`admit`] let the call in.
async fn rpc(State(state): State<Shared>, headers: HeaderMap, body: String) -> Response {
    let Some(cfg) = state.a2a.as_ref() else {
        return rpc_err(Value::Null, -32603, "not exposed").into_response(); // LCOV_EXCL_LINE the route is mounted only with the file (lib.rs)
    };
    if let Some(refused) = admit(&headers, cfg) {
        return refused;
    }
    let req: Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(_) => return rpc_err(Value::Null, -32700, "parse error").into_response(),
    };
    let id = req.get("id").cloned().unwrap_or(Value::Null);
    if req.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return rpc_err(id, -32600, "invalid request: jsonrpc must be \"2.0\"").into_response();
    }
    let Some(method) = req.get("method").and_then(Value::as_str) else {
        return rpc_err(id, -32600, "invalid request: missing method").into_response();
    };
    let params = req.get("params").cloned().unwrap_or(Value::Null);
    match method {
        "message/send" => message_send(&state, id, params).await,
        "tasks/get" => tasks_get(&state, id, params),
        "tasks/cancel" => tasks_cancel(&state, id, params),
        _ => rpc_err(id, -32601, &format!("method not found: {method}")),
    }
    .into_response()
}

// ---------------------------------------------------------------------------
// message/send
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct SendParams {
    message: IncomingMessage,
}

#[derive(Deserialize)]
struct IncomingMessage {
    #[serde(default)]
    parts: Vec<Value>,
    #[serde(default, rename = "taskId")]
    task_id: Option<String>,
}

fn text_of_parts(parts: &[Value]) -> String {
    parts
        .iter()
        .filter(|p| p.get("kind").and_then(Value::as_str) == Some("text"))
        .filter_map(|p| p.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

async fn message_send(state: &Shared, id: Value, params: Value) -> Json<Value> {
    let Some(cfg) = state.a2a.clone() else {
        return rpc_err(id, -32603, "a2a not configured"); // LCOV_EXCL_LINE: the routes are mounted with a configuration; by construction
    };
    let params: SendParams = match serde_json::from_value(params) {
        Ok(p) => p,
        Err(e) => return rpc_err(id, -32602, &format!("invalid params: {e}")),
    };
    let text = text_of_parts(&params.message.parts);

    // Continuation: the message lands in the existing task's goal journal.
    if let Some(task_id) = params.message.task_id.as_deref() {
        let Some(goal) = find_task(state, task_id) else {
            return rpc_err(id, TASK_NOT_FOUND, "task not found");
        };
        if !text.is_empty() {
            if let Err(e) = bisa_engine::ops::add_note(
                state.engine.inner(),
                goal.id,
                format!("A2A caller: {text}"),
                None,
            ) {
                // LCOV_EXCL_START journaling fails only unwritable (disk-only)
                tracing::warn!("goal {}: could not journal the A2A message: {e}", goal.id);
                // LCOV_EXCL_STOP
            }
        } // LCOV_EXCL_LINE the other edge of the branch; by construction
        return rpc_ok(id, task_json(state, &goal));
    }

    if text.is_empty() {
        return rpc_err(id, -32602, "invalid params: message has no text parts");
    }

    // New task: a goal — on the configured workflow, started when
    // `auto_start` says so; designed otherwise, so the Workflow Agent designs
    // its workflow and a person adopts it. Either begins only once the
    // content screen passed the caller's words (or a person let them
    // through): until then the goal is a manual draft that wakes nobody and
    // runs nothing, tagged as A2A's own.
    let title: String = text.lines().next().unwrap_or("").chars().take(80).collect();
    let text_for_signal = text.clone();
    let goal = match state.engine.submit_goal(SubmitRequest {
        statement: text,
        title: Some(title),
        budget: Default::default(),
        mode: bisa_core::GoalMode::Manual,
        origin: bisa_core::GoalOrigin::Captured,
        workflow: cfg.workflow,
        inputs: Default::default(),
        start: false,
        assignees: vec![],
        tags: Tags::new([A2A_TAG]).expect("a tag the platform spells"),
        documents: vec![],
    }) {
        Ok(g) => g,
        Err(e) => return rpc_err(id, -32603, &e.to_string()), // LCOV_EXCL_LINE a goal unreadable under its task; disk-only
    };
    // What the screen's pass does with the goal: without a workflow the
    // Workflow Agent designs one and the platform runs it — a remote caller
    // is not here to adopt; with one, the workflow is the whole
    // clarification, begun at once when `auto_start` says so.
    let then = OutsideGoal {
        goal: goal.id,
        mode: if cfg.workflow.is_none() {
            bisa_core::GoalMode::Auto
        } else {
            bisa_core::GoalMode::Manual
        },
        start: cfg.workflow.is_some() && cfg.auto_start,
    };
    if let Err(e) = bisa_engine::ops::add_note(
        state.engine.inner(),
        goal.id,
        "created via A2A message/send".into(),
        None,
    ) {
        // LCOV_EXCL_START journaling fails only unwritable (disk-only)
        tracing::warn!("goal {}: could not journal the A2A note: {e}", goal.id);
        // LCOV_EXCL_STOP
    }
    // "An external system contributed" is one fact whichever door it came
    // through: a hook call, a session's tool and an A2A task all become a
    // signal — so a start or a wait that names `a2a.task` hears a remote
    // agent's request as it hears any other. This one came from outside, so
    // it is raised through the door that reads it first: redacted, and held
    // until the content screen passes it or a person lets it through.
    // Best-effort — a caller's task must not fail because nothing listened.
    if let Err(e) = state.engine.emit_outside_signal_for_goal(
        A2A_TASK_SIGNAL,
        json!({"task": goal.id.to_string(), "goal": goal.id.to_string(), "text": text_for_signal}),
        bisa_core::SignalScope::Goal { goal: goal.id },
        A2A_SOURCE,
        then,
    ) {
        tracing::warn!("a2a task signal: {e}"); // LCOV_EXCL_LINE a signal the engine cannot take; disk-only
    }
    match state.engine.workspace().get_goal(goal.id) {
        Ok(goal) => rpc_ok(id, task_json(state, &goal)),
        Err(e) => rpc_err(id, -32603, &e.to_string()), // LCOV_EXCL_LINE a goal unreadable under its task; disk-only
    }
}

// ---------------------------------------------------------------------------
// tasks/get, tasks/cancel
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct TaskIdParams {
    id: String,
}

fn tasks_get(state: &Shared, id: Value, params: Value) -> Json<Value> {
    let params: TaskIdParams = match serde_json::from_value(params) {
        Ok(p) => p,
        Err(e) => return rpc_err(id, -32602, &format!("invalid params: {e}")),
    };
    match find_task(state, &params.id) {
        Some(goal) => rpc_ok(id, task_json(state, &goal)),
        None => rpc_err(id, TASK_NOT_FOUND, "task not found"),
    }
}

fn tasks_cancel(state: &Shared, id: Value, params: Value) -> Json<Value> {
    let params: TaskIdParams = match serde_json::from_value(params) {
        Ok(p) => p,
        Err(e) => return rpc_err(id, -32602, &format!("invalid params: {e}")),
    };
    let Some(goal) = find_task(state, &params.id) else {
        return rpc_err(id, TASK_NOT_FOUND, "task not found");
    };
    let run = run_of(state, &goal);
    let status = goal.status(run.as_ref());
    if matches!(
        status,
        GoalStatus::Done | GoalStatus::Failed | GoalStatus::Closed
    ) {
        return rpc_err(id, TASK_NOT_CANCELABLE, "task already terminal");
    }
    // Closing is the one move a goal makes on its own: it cancels the run.
    let goal = match state.engine.close_goal(
        goal.id,
        ClosureReason::Abandoned {
            rationale: Some("cancelled by the A2A caller".into()),
        },
    ) {
        Ok(g) => g,
        Err(e) => return rpc_err(id, -32603, &e.to_string()), // LCOV_EXCL_LINE a goal unreadable under its task; disk-only
    };
    rpc_ok(id, task_json(state, &goal))
}

// ---------------------------------------------------------------------------
// Task mapping
// ---------------------------------------------------------------------------

/// The goal a task id names — when A2A made it ([`A2A_TAG`]); any other
/// goal of the workspace is no task of the caller's.
fn find_task(state: &Shared, task_id: &str) -> Option<Goal> {
    let id = GoalId::from_str(task_id).ok()?;
    state
        .engine
        .workspace()
        .get_goal(id)
        .ok()
        .filter(|goal| goal.tags.contains(A2A_TAG))
}

fn run_of(state: &Shared, goal: &Goal) -> Option<WorkflowRun> {
    goal.run
        .and_then(|r| state.engine.workspace().get_run(r).ok())
}

fn a2a_state(state: &Shared, goal: &Goal, run: Option<&WorkflowRun>) -> &'static str {
    match goal.status(run) {
        // A draft owes somebody something: an adoption to decide, or a run
        // to start. Only a draft with no workflow at all — the Workflow Agent
        // still designing — is merely `submitted`.
        GoalStatus::Draft => {
            let owed = goal.workflow.is_some()
                || state
                    .engine
                    .inbox()
                    .iter()
                    .any(|g| g.home.goal() == Some(goal.id));
            if owed {
                "input-required"
            } else {
                "submitted"
            }
        }
        GoalStatus::Running => "working",
        GoalStatus::Waiting => "input-required",
        // LCOV_EXCL_START a task of a goal that ended, which the feature's rig does not stage; out by decision
        GoalStatus::Done => "completed",
        GoalStatus::Failed => "failed",
        // LCOV_EXCL_STOP
        GoalStatus::Closed => "canceled",
    }
}

fn task_json(state: &Shared, goal: &Goal) -> Value {
    let run = run_of(state, goal);
    let task_state = a2a_state(state, goal, run.as_ref());
    let mut artifacts: Vec<Value> = Vec::new();
    if let Some(run) = run
        .as_ref()
        .filter(|r| r.outcome == Some(bisa_core::RunOutcome::Done))
    {
        // Every step's output, in workflow order, is the artifact.
        // LCOV_EXCL_START a task's artifacts from a finished run, which the feature's rig does not stage; out by decision
        for step in &run.workflow.steps {
            let Some(output) = run.steps.get(&step.id).and_then(|r| r.output.clone()) else {
                continue;
            };
            artifacts.push(json!({
                "artifactId": format!("{}-{}", run.id, step.id),
                "name": step.id,
                "parts": [
                    {"kind": "data", "data": output},
                    {"kind": "text", "text": format!("step `{}` ({}) done", step.id, step.kind.as_str())},
        // LCOV_EXCL_STOP
                ],
            }));
        }
    }
    json!({
        "id": goal.id.to_string(),
        "contextId": goal.id.to_string(),
        "status": {"state": task_state},
        "artifacts": artifacts,
        "kind": "task",
    })
}

/// The routes this module mounts, for `bisa-node --bin api-docs` and the
/// route test. Kept beside `routes()` so a route added here is added here.
pub const ROUTES: &[RouteDoc] = &[
    RouteDoc {
        method: "GET",
        path: "/.well-known/agent-card.json",
        summary: "The A2A agent card describing this node — with `securitySchemes` and `security` when `a2a.toml` names a `token`.",
    },
    RouteDoc {
        method: "POST",
        path: "/a2a",
        summary: "The A2A JSON-RPC endpoint; malformed JSON answers with a JSON-RPC error. Before the body is read: a call carrying an `Origin` header is `403`, one that is not `application/json` is `415`, and with a `token` configured a missing or wrong `Authorization: Bearer` is `401`. `tasks/get`, `tasks/cancel` and a continuation reach only the goals A2A made; a new task's goal begins once the content screen passed its words or a person released the held signal.",
    },
];
