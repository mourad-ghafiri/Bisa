//! Listening over HTTP: who hears a workflow's start events, what each
//! listener is doing, the local door a `hook` start is called through, and
//! the signals the runtime keeps.
//!
//! A **listener** is a start event armed for a **host** — a library workflow
//! a person turned On (`workspace:<WorkflowId>`), or a goal whose workflow
//! begins on events (`goal:<GoalId>`). Three rules shape every route here.
//!
//! **An event never acts — it enqueues.** Nothing in this module starts a
//! run. A hook call and a raised signal are written to the durable queue by
//! the engine ([`bisa_engine::Engine::call_hook`],
//! [`bisa_engine::Engine::emit_signal`]) and answered with the signal; the
//! engine's worker starts a run from it under the same gates, budgets and
//! concurrency cap as a person's start. Whether an occurrence starts its run
//! — the start's guard, the causal chain, the backlog — is decided once, at
//! dispatch, in the engine.
//!
//! **Turning a host on is the engine's judgement.** `PUT …/listening` hands
//! the inputs and the budget to [`bisa_engine::Engine::set_listening`], which
//! refuses a workflow with problems (the body carries them), one put away,
//! one that reads a goal it would not have, a goal's own design, and inputs
//! that do not bind what its events need. A toggle is no revision of the
//! definition.
//!
//! **A hook's secret is shown once and never again.** The turn that mints it
//! answers with it (`secrets`), and so does a rotation; no read route returns
//! it — a listener's view says only whether one is minted — and it is in no
//! record, snapshot or index. Losing it is recovered by
//! `POST …/hooks/{step}/secret`, which mints a new one and invalidates the
//! old.
//!
//! A [`ListenerView`] is built in one place ([`view`]): what the registry
//! armed of a start, and what the store remembers of it. [`listeners_of`]
//! and [`every_listener`] read a host's and every host's off an engine, and
//! are public: the command line lists the same views when it reads the
//! workspace itself.

use crate::dto::*;
use crate::hooks;
use crate::route_docs::RouteDoc;
use crate::runs::parse_step;
use crate::workflows::{existing, parse_wfid};
use crate::Query;
use crate::{bad_request, not_found, parse_id, ApiError, Shared};
use axum::body::Body;
use axum::extract::{Path as AxPath, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use bisa_core::signal::MAX_SIGNAL_PAYLOAD_BYTES;
use bisa_core::{GoalId, ListenerHost, ListenerKey, SignalScope, StartOn, Workflow, WorkflowId};
use bisa_engine::listen::registry::Registry;
use bisa_engine::{Engine, HookDoor};
use bisa_store::{StoreError, Workspace};
use serde::Deserialize;
use serde_json::json;

/// How many signals `GET /signals` answers when it is not told.
const DEFAULT_SIGNAL_LIMIT: usize = 50;
/// The most it answers, whatever it is told.
const MAX_SIGNAL_LIMIT: usize = 200;

pub(crate) fn routes() -> Router<Shared> {
    Router::new()
        .route(
            "/workflows/{wfid}/listening",
            put(turn_on_workflow).delete(turn_off_workflow),
        )
        .route("/workflows/{wfid}/listeners", get(workflow_listeners))
        .route("/workflows/{wfid}/hooks/{step}", post(call_workflow_hook))
        .route(
            "/workflows/{wfid}/hooks/{step}/secret",
            post(rotate_workflow_secret),
        )
        .route(
            "/goals/{id}/listening",
            put(listen_goal).delete(stop_listening_goal),
        )
        .route("/goals/{id}/listeners", get(goal_listeners))
        .route("/goals/{id}/hooks/{step}", post(call_goal_hook))
        .route("/goals/{id}/hooks/{step}/secret", post(rotate_goal_secret))
        .route("/listeners", get(listeners))
        .route("/signals", get(signals).post(emit))
        .route("/signals/{id}/release", post(release))
}

// ---------------------------------------------------------------------------
// Hosts and listeners, read off the wire
// ---------------------------------------------------------------------------

/// The library workflow a route names, as a host — or its 404.
fn workflow_host(state: &Shared, wfid: &str) -> Result<(WorkflowId, ListenerHost), ApiError> {
    let workflow = parse_wfid(wfid)?;
    existing(state, workflow)?;
    Ok((workflow, ListenerHost::Workspace { workflow }))
}

/// The goal a route names, as a host — or its 404.
fn goal_host(state: &Shared, id: &str) -> Result<(GoalId, ListenerHost), ApiError> {
    let goal = parse_id(id)?;
    state.engine.workspace().get_goal(goal)?;
    Ok((goal, ListenerHost::Goal { goal }))
}

/// The listener a hook route names: a step of a host.
fn listener(host: ListenerHost, step: &str) -> Result<ListenerKey, ApiError> {
    Ok(ListenerKey {
        host,
        step: parse_step(step)?,
    })
}

// ---------------------------------------------------------------------------
// The listener's view
// ---------------------------------------------------------------------------

/// The workflow a host listens with: a library workflow itself, a goal's the
/// one it runs — none for a goal that has none.
fn workflow_of(ws: &Workspace, host: &ListenerHost) -> Result<Option<Workflow>, StoreError> {
    match host {
        ListenerHost::Workspace { workflow } => Ok(Some(ws.get_workflow(*workflow)?)),
        ListenerHost::Goal { goal } => match ws.get_goal(*goal)?.workflow {
            Some(workflow) => Ok(Some(ws.get_workflow(workflow)?)),
            None => Ok(None),
        },
    }
}

/// One listener as the routes list it — the one place its view is built:
/// the start its step declares, what the registry armed of it, and what the
/// store remembers. The keystore is asked about a public hook's secret and
/// about nothing else.
pub fn view(
    ws: &Workspace,
    registry: &Registry,
    key: ListenerKey,
    declared: &StartOn,
) -> Result<ListenerView, StoreError> {
    let armed = registry.get(&key).map(|armed| armed.on.clone());
    let public_hook = matches!(
        armed.as_ref().unwrap_or(declared),
        StartOn::Hook { public: true }
    );
    let facts = ListenerFacts {
        runtime: ws.listener_runtime(&key),
        last_signal_at: ws.last_signal_at(&key)?,
        backlog: ws.pending_signals(&key)?.len(),
        live_runs: ws.live_runs_of_listener(&key)?,
        has_secret: public_hook && ws.has_hook_secret(&key)?,
        armed,
    };
    Ok(ListenerView::of(key, declared, facts))
}

/// The listeners of one host: every start of its workflow that begins on an
/// event, in definition order, while the host listens — none while it does
/// not. A start the registry could not arm is listed too, saying why. A host
/// nobody has is refused in the store's words.
pub fn listeners_of(engine: &Engine, host: &ListenerHost) -> Result<Vec<ListenerView>, StoreError> {
    let ws = engine.workspace();
    let workflow = workflow_of(ws, host)?;
    if ws.listening(host)?.is_none() {
        return Ok(Vec::new());
    }
    let Some(workflow) = workflow else {
        return Ok(Vec::new());
    };
    let registry = engine.armed_listeners();
    workflow
        .event_starts()
        .into_iter()
        .map(|(step, declared)| {
            let key = ListenerKey {
                host: *host,
                step: step.id.clone(),
            };
            view(ws, &registry, key, declared)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// A library workflow: On and Off
// ---------------------------------------------------------------------------

/// Turn a library workflow On: `{inputs?, budget?}` →
/// `{workflow, listeners, secrets}`. The secrets are the public hooks' this
/// turn minted — shown here, once.
async fn turn_on_workflow(
    State(state): State<Shared>,
    AxPath(wfid): AxPath<String>,
    crate::Body(body): crate::Body<ListeningBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let (workflow, host) = workflow_host(&state, &wfid)?;
    let turned = state.engine.set_listening(host, body.inputs, body.budget)?;
    Ok(Json(json!({
        "workflow": workflow_row(&state, workflow)?,
        "listeners": listeners_of(&state.engine, &host)?,
        "secrets": turned.secrets,
    })))
}

/// Turn a library workflow Off → `{workflow}`. What its events had queued
/// settles unheard; Off already is nothing to do.
async fn turn_off_workflow(
    State(state): State<Shared>,
    AxPath(wfid): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let (workflow, host) = workflow_host(&state, &wfid)?;
    state.engine.stop_listening(host)?;
    Ok(Json(json!({"workflow": workflow_row(&state, workflow)?})))
}

/// The workflow as its row reads now — what a toggle answers with.
fn workflow_row(state: &Shared, workflow: WorkflowId) -> Result<WorkflowRow, ApiError> {
    crate::workflows::row(state, existing(state, workflow)?)
}

async fn workflow_listeners(
    State(state): State<Shared>,
    AxPath(wfid): AxPath<String>,
) -> Result<Json<Vec<ListenerView>>, ApiError> {
    let (_, host) = workflow_host(&state, &wfid)?;
    Ok(Json(listeners_of(&state.engine, &host)?))
}

// ---------------------------------------------------------------------------
// A goal: listening, and listening again
// ---------------------------------------------------------------------------

/// The goal listens — or listens again, after a failed run or a spent budget
/// paused it: `{inputs?}` → `{goal, listeners, secrets}`.
async fn listen_goal(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
    crate::Body(body): crate::Body<GoalListeningBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let (goal, host) = goal_host(&state, &id)?;
    let turned = state.engine.listen_again(goal, body.inputs)?;
    Ok(Json(json!({
        "goal": crate::goals::view(&state, goal)?,
        "listeners": listeners_of(&state.engine, &host)?,
        "secrets": turned.secrets,
    })))
}

/// The goal stops listening → `{goal}`. A live run of it goes on.
async fn stop_listening_goal(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let (goal, host) = goal_host(&state, &id)?;
    state.engine.stop_listening(host)?;
    Ok(Json(json!({"goal": crate::goals::view(&state, goal)?})))
}

async fn goal_listeners(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<Json<Vec<ListenerView>>, ApiError> {
    let (_, host) = goal_host(&state, &id)?;
    Ok(Json(listeners_of(&state.engine, &host)?))
}

// ---------------------------------------------------------------------------
// Every listener
// ---------------------------------------------------------------------------

/// Every listener of every host that listens: the library's, then the
/// goals'. A host whose listeners cannot be read is left out and said in the
/// log — one broken host never costs the list.
pub fn every_listener(engine: &Engine) -> Result<Vec<ListenerView>, StoreError> {
    let mut out = Vec::new();
    for (host, _) in engine.workspace().list_listening()? {
        match listeners_of(engine, &host) {
            Ok(views) => out.extend(views),
            Err(e) => {
                tracing::warn!(target: "bisa_node", %host, "a listening host's listeners cannot be read; it is left out: {e}")
            }
        }
    }
    Ok(out)
}

async fn listeners(State(state): State<Shared>) -> Result<Json<Vec<ListenerView>>, ApiError> {
    Ok(Json(every_listener(&state.engine)?))
}

// ---------------------------------------------------------------------------
// The local hook door
// ---------------------------------------------------------------------------

/// One call to a hook start from this machine, under the control-plane
/// token: the body is the payload, a delivery id makes a redelivery the same
/// signal → `202 {signal}`. The engine decides whether the listener takes
/// it; a refusal is said in words (`hooks::refused_locally`).
async fn call_hook(
    state: &Shared,
    key: ListenerKey,
    headers: HeaderMap,
    body: Body,
) -> Result<Response, ApiError> {
    let raw = hooks::read_body(&headers, body)
        .await
        .map_err(|_| hooks::too_large_locally())?;
    let payload = hooks::payload_of(&raw).map_err(|_| hooks::too_large_locally())?;
    let delivery = hooks::delivery_of(&headers);
    let signal = state
        .engine
        .call_hook(&key, payload, delivery.as_deref(), HookDoor::Local)
        .map_err(hooks::refused_locally)?;
    Ok(hooks::accepted(signal))
}

async fn call_workflow_hook(
    State(state): State<Shared>,
    AxPath((wfid, step)): AxPath<(String, String)>,
    headers: HeaderMap,
    body: Body,
) -> Result<Response, ApiError> {
    let (_, host) = workflow_host(&state, &wfid)?;
    call_hook(&state, listener(host, &step)?, headers, body).await
}

async fn call_goal_hook(
    State(state): State<Shared>,
    AxPath((id, step)): AxPath<(String, String)>,
    headers: HeaderMap,
    body: Body,
) -> Result<Response, ApiError> {
    let (_, host) = goal_host(&state, &id)?;
    call_hook(&state, listener(host, &step)?, headers, body).await
}

/// Mint a public hook start's secret anew → `HookSecret`, shown this once.
/// The recovery path for a secret that was lost — there is no other.
async fn rotate_workflow_secret(
    State(state): State<Shared>,
    AxPath((wfid, step)): AxPath<(String, String)>,
) -> Result<Json<bisa_engine::HookSecret>, ApiError> {
    let (_, host) = workflow_host(&state, &wfid)?;
    let key = listener(host, &step)?;
    Ok(Json(state.engine.rotate_hook_secret(&key)?))
}

async fn rotate_goal_secret(
    State(state): State<Shared>,
    AxPath((id, step)): AxPath<(String, String)>,
) -> Result<Json<bisa_engine::HookSecret>, ApiError> {
    let (_, host) = goal_host(&state, &id)?;
    let key = listener(host, &step)?;
    Ok(Json(state.engine.rotate_hook_secret(&key)?))
}

// ---------------------------------------------------------------------------
// Signals
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct SignalsQuery {
    #[serde(default)]
    limit: Option<usize>,
    /// One host's signals: `workspace:<WorkflowId>` or `goal:<GoalId>`.
    #[serde(default)]
    host: Option<String>,
}

impl SignalsQuery {
    fn host(&self) -> Result<Option<ListenerHost>, ApiError> {
        self.host
            .as_deref()
            .map(|raw| {
                raw.parse::<ListenerHost>().map_err(|_| {
                    bad_request(bisa_core::text!(
                        "error-node-listening-not-a-host",
                        raw = format!("{raw:?}")
                    ))
                })
            })
            .transpose()
    }
}

/// The newest signals, queued or settled — of one host (`?host=`), or of
/// every one and of none: a named signal kept for the waits that replay it
/// names no listener.
async fn signals(
    State(state): State<Shared>,
    Query(q): Query<SignalsQuery>,
) -> Result<Json<Vec<SignalView>>, ApiError> {
    let host = q.host()?;
    let limit = q
        .limit
        .unwrap_or(DEFAULT_SIGNAL_LIMIT)
        .clamp(1, MAX_SIGNAL_LIMIT);
    let rows = state
        .engine
        .workspace()
        .list_signals(host.as_ref(), limit)?;
    Ok(Json(rows.into_iter().map(SignalView::from).collect()))
}

/// A payload the queue would refuse, refused here as the body it is: 413.
fn payload_fits(payload: &serde_json::Value) -> Result<(), ApiError> {
    let len = serde_json::to_vec(payload).map_or(usize::MAX, |bytes| bytes.len());
    if len > MAX_SIGNAL_PAYLOAD_BYTES {
        return Err(ApiError::text(
            StatusCode::PAYLOAD_TOO_LARGE,
            bisa_core::text!(
                "error-node-listening-signal-payload-over-cap",
                len = len.to_string(),
                max = MAX_SIGNAL_PAYLOAD_BYTES.to_string()
            ),
        ));
    }
    Ok(())
}

/// Raise a named signal by hand: `{name, payload?, goal?}` →
/// `{signal, listeners}` — the record kept for the waits that replay it, and
/// the signals written for the listeners that heard it. Nothing listening is
/// a normal answer, never an error.
async fn emit(
    State(state): State<Shared>,
    crate::Body(body): crate::Body<EmitSignalBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    payload_fits(&body.payload)?;
    let scope = match body.goal {
        Some(goal) => {
            state.engine.workspace().get_goal(goal)?;
            SignalScope::Goal { goal }
        }
        None => SignalScope::Workspace,
    };
    let emitted = state.engine.emit_signal(&body.name, body.payload, scope)?;
    Ok(Json(json!({
        "signal": emitted.signal,
        "listeners": emitted.listeners,
    })))
}

/// Let a held signal through — a person read what the content screen would
/// not pass → `{signal}`, queued again. 404 for an id that names none, 409
/// for one that is not held.
async fn release(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let ws = state.engine.workspace();
    if ws.signal(&id)?.is_none() {
        return Err(not_found(bisa_core::text!(
            "error-node-listening-signal-not-found",
            id = id.clone()
        )));
    }
    state.engine.release_signal(&id)?;
    let signal = ws.signal(&id)?.map(SignalView::from);
    Ok(Json(json!({"signal": signal})))
}

/// The routes this module mounts, for `bisa-node --bin api-docs` and the
/// route test. Kept beside `routes()` so a route added here is added here.
pub const ROUTES: &[RouteDoc] = &[
    RouteDoc { method: "PUT", path: "/workflows/{wfid}/listening", summary: "Turn a library workflow On: `{inputs?, budget?}` → `{workflow, listeners, secrets}` — its start events are heard from now on, each occurrence a run of the workspace binding `inputs` beneath what the start's mapping reads off the event, under `budget` (absent, `budget.default.*`; `{}`, no ceiling). `workflow` is its row, `listeners` a `ListenerView` per event start, `secrets` the public hooks' secrets this turn minted (`{step, path, secret}`, hex) — **shown once**. 400 with its `problems` while it has any or reads a goal it would not have (`needs_goal`); 400 when it is archived, is a goal's own design, has no start on an event, an input is unknown or of the wrong kind, `listening_needs` does not bind, or a `check` start's command is refused by the guard. A toggle is no revision of the definition. The bus hears `listening_changed`." },
    RouteDoc { method: "DELETE", path: "/workflows/{wfid}/listening", summary: "Turn a library workflow Off → `{workflow}` (its row): its start events are no longer heard and what they had queued settles `skipped`; a run already going goes on; a hook's secret is kept for the next On. Off already is nothing to do." },
    RouteDoc { method: "GET", path: "/workflows/{wfid}/listeners", summary: "The workflow's listeners while it is On — `[]` while it is Off: a `ListenerView` per event start, in definition order — `{listener, host, step, event, summary, next_due?, last_fired_at?, backlog, live_runs, local_hook?, public_hook?: {path, has_secret}, failed?}`. A start reads as it is armed (every input read); one that could not be armed is listed with why (`failed`). Never a secret." },
    RouteDoc { method: "POST", path: "/workflows/{wfid}/hooks/{step}", summary: "Call a `hook` start of a library workflow from this machine, under the control-plane token: the body is the payload (a JSON object as it comes, any other JSON under `value`, a body that is not JSON under `text`; at most 64 KiB), `Idempotency-Key` or `X-GitHub-Delivery` makes a redelivery the same signal → `202 {signal}`. Nothing runs on this request. 409 while the workflow is Off or `events.enabled` is off, 404 for a step that is no hook start of it, 413, 429 when its backlog is full." },
    RouteDoc { method: "POST", path: "/workflows/{wfid}/hooks/{step}/secret", summary: "Mint a public hook start's secret anew → `{step, path, secret}` (hex), **shown once**; the old one stops verifying at once. 400 for a step that is no public hook start." },
    RouteDoc { method: "PUT", path: "/goals/{id}/listening", summary: "The goal listens — or listens again after a failed run or a spent budget paused it: `{inputs?}` (absent, what it listened with before) → `{goal, listeners, secrets}` — `goal` as `GET /goals/{id}` answers it, `secrets` the public hooks' secrets this turn minted, shown once. Each occurrence starts a run on the goal, queued behind a live one; its runs spend against the goal's budget. Refused as turning a workflow On is, and 409 for a closed goal. The bus hears `listening_changed`." },
    RouteDoc { method: "DELETE", path: "/goals/{id}/listening", summary: "The goal stops listening → `{goal}` (as `GET /goals/{id}` answers it): its start events are no longer heard and what they had queued settles `skipped`; a live run goes on." },
    RouteDoc { method: "GET", path: "/goals/{id}/listeners", summary: "The goal's listeners while it listens — `[]` otherwise: a `ListenerView` per event start of its workflow." },
    RouteDoc { method: "POST", path: "/goals/{id}/hooks/{step}", summary: "Call a `hook` start of the goal's workflow from this machine, as `POST /workflows/{wfid}/hooks/{step}` does → `202 {signal}`; 409 while the goal does not listen or is paused." },
    RouteDoc { method: "POST", path: "/goals/{id}/hooks/{step}/secret", summary: "Mint a public hook start's secret of the goal anew → `{step, path, secret}`, shown once." },
    RouteDoc { method: "GET", path: "/listeners", summary: "Every listener of every host that listens — the library workflows that are On, then the goals: `ListenerView[]`. A host whose listeners cannot be read is left out, never the reason the list fails." },
    RouteDoc { method: "GET", path: "/signals", summary: "The newest signals, queued or settled (`?limit=`, 50 unless said and 200 at most; `?host=workspace:<WorkflowId>|goal:<GoalId>` for one host's): `SignalView[]` — `{id, listener?, source, name?, at, state, scope, note?}`, `state` one of `queued` · `running` · `waiting` · `held` · `done` · `skipped` · `failed`, `note` why when it says. Never a payload." },
    RouteDoc { method: "POST", path: "/signals", summary: "Raise a named signal: `{name, payload?, goal?}` → `{signal, listeners}` — `signal` the record kept for the waits that replay it, `listeners` the signals written for the `signal` starts that heard it; on the goal when `goal` names one, in the workspace otherwise. Nothing listening is a normal answer. 400 for a name that is not dot-separated lowercase parts, 404 for an unknown goal, 413 for a payload over 64 KiB." },
    RouteDoc { method: "POST", path: "/signals/{id}/release", summary: "Let a held signal through — an outside payload the content screen would not pass, read by a person → `{signal}` (a `SignalView`, queued again). 404 for an id that names no signal, 409 for one that is not held." },
];

#[cfg(test)]
mod tests {
    use super::*;

    /// The refusal of a result that had to be one — `ApiError` is not `Debug`.
    fn refusal<T>(result: Result<T, ApiError>) -> ApiError {
        match result {
            Err(e) => e,
            Ok(_) => panic!("taken, where a refusal was due"),
        }
    }

    #[test]
    fn a_signals_host_is_a_listener_host_or_refused_by_name() {
        let none = SignalsQuery {
            limit: None,
            host: None,
        };
        assert_eq!(none.host().ok(), Some(None));
        let workflow = bisa_core::WorkflowId::from_ulid(ulid::Ulid::from_parts(1, 1));
        let one = SignalsQuery {
            limit: None,
            host: Some(format!("workspace:{workflow}")),
        };
        assert_eq!(
            one.host().ok(),
            Some(Some(ListenerHost::Workspace { workflow }))
        );
        let wrong = SignalsQuery {
            limit: None,
            host: Some("everything".into()),
        };
        let refused = refusal(wrong.host());
        assert_eq!(refused.status, StatusCode::BAD_REQUEST);
        assert!(
            refused.text.to_string().contains("everything"),
            "{}",
            refused.text
        );
    }

    #[test]
    fn a_payload_over_the_cap_is_a_413_that_names_both_sizes() {
        assert!(payload_fits(&json!({"n": 1})).is_ok());
        assert!(payload_fits(&serde_json::Value::Null).is_ok());
        let big = json!({ "blob": "x".repeat(MAX_SIGNAL_PAYLOAD_BYTES) });
        let refused = refusal(payload_fits(&big));
        assert_eq!(refused.status, StatusCode::PAYLOAD_TOO_LARGE);
        assert!(
            refused
                .text
                .to_string()
                .contains(&MAX_SIGNAL_PAYLOAD_BYTES.to_string()),
            "{}",
            refused.text
        );
    }

    #[test]
    fn a_listener_is_a_step_of_its_host() {
        let workflow = bisa_core::WorkflowId::from_ulid(ulid::Ulid::from_parts(2, 2));
        let host = ListenerHost::Workspace { workflow };
        let Ok(key) = listener(host, "ticket") else {
            panic!("`ticket` is a step id");
        };
        assert_eq!(key.to_string(), format!("workspace:{workflow}/ticket"));
        assert_eq!(
            refusal(listener(host, "Not A Step")).status,
            StatusCode::BAD_REQUEST
        );
    }
}
