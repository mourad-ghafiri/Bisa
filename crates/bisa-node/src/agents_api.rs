//! Agents (definitions: prompt + harness + model plan + skill and MCP
//! references), Teams (agents + humans), Recall (owner-readable agent
//! memory), and the runtime session roster (renamed from the old `/agents`).
//!
//! **An agent references skills and MCP servers; it does not contain them.**
//! There used to be a `POST /agents/{id}/skills` that took markdown, which
//! meant twenty agents following one checklist were twenty copies drifting
//! apart. The library ([`crate::skills`]) and the registry ([`crate::mcp`])
//! hold the content now, and the wiring routes here are the whole vocabulary:
//! attach an id, detach an id.

use crate::dto::*;
use crate::route_docs::RouteDoc;
use crate::tags::{parse_tags, TagFilter};
use crate::{agent_id, bad_request, mcp_id, not_found, skill_id, team_id, ApiError, Shared};
use axum::extract::{Path as AxPath, State};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::routing::{get, post};
use axum::{Json, Router};
use bisa_core::tags::TagEntity;
use bisa_core::Localize as _;
use bisa_core::{Assignee, McpId, PhotoProfile, RespondPolicy, SkillId, Team};
use bisa_store::{NewAgent, Workspace};
use serde_json::json;
use std::str::FromStr;

pub(crate) fn routes() -> Router<Shared> {
    Router::new()
        .route("/agents", get(list_agents).post(create_agent))
        .route(
            "/agents/{id}",
            get(get_agent).patch(patch_agent).delete(delete_agent),
        )
        .route("/agents/{id}/recall", get(agent_recall))
        .route(
            "/agents/{id}/skills/{skill_id}",
            post(attach_skill).delete(detach_skill),
        )
        .route(
            "/agents/{id}/mcps/{mcp_id}",
            post(attach_mcp).delete(detach_mcp),
        )
        .route("/harnesses/{id}/models", get(harness_models))
        .route("/models/health", get(models_health))
        .route("/teams", get(list_teams).post(create_team))
        .route(
            "/teams/{id}",
            get(get_team).patch(patch_team).delete(delete_team),
        )
        .route("/sessions", get(sessions))
        .route("/sessions/terminal", post(open_terminal_session))
        .route("/sessions/{id}", get(session))
        .route("/sessions/{id}/abort", post(abort_session))
        .route("/sessions/{id}/report", post(report_session))
        .route("/sessions/{id}/guard", post(guard_session))
        .route("/sessions/{id}/exit", post(exit_session))
        .route("/sessions/{id}/close", post(close_session))
        .route("/sessions/{id}/answered", post(answered_session))
}

fn parse_respond(s: Option<&str>) -> Result<RespondPolicy, ApiError> {
    match s.unwrap_or("owner_only") {
        "owner_only" => Ok(RespondPolicy::OwnerOnly),
        "members" => Ok(RespondPolicy::Members),
        other => Err(bad_request(bisa_core::text!(
            "error-node-agents_api-unknown-respond-policy",
            other = format!("{other:?}")
        ))),
    }
}

/// Whether the library holds what a reference names. What is not there is
/// the caller's to fix, in the words `unknown` gives; a record that is there
/// and cannot be read is the store's own answer, naming its file — never
/// *unknown*, which would send a person looking for a typo.
fn held<T>(
    read: Result<T, bisa_store::StoreError>,
    unknown: impl FnOnce() -> bisa_core::Text,
) -> Result<(), ApiError> {
    match read {
        Ok(_) => Ok(()),
        Err(bisa_store::StoreError::DefinitionNotFound { .. }) => Err(bad_request(unknown())),
        Err(e) => Err(e.into()),
    }
}

/// Refuse a reference to something that is not there, at the moment it is
/// written.
///
/// The store tolerates a dangling id at launch — a vanished skill costs that
/// skill, never the session — but tolerating one *here* would turn a typo
/// into an agent that quietly runs without the procedure it was given.
/// Every referenced skill and MCP server must exist. Returns the typed ids.
fn check_refs(
    ws: &Workspace,
    skills: &[String],
    mcps: &[String],
) -> Result<(Vec<SkillId>, Vec<McpId>), ApiError> {
    let mut skill_ids = Vec::with_capacity(skills.len());
    for id in skills {
        let sid = skill_id(id)?;
        held(ws.get_skill(&sid), || {
            bisa_core::text!("error-node-agents_api-unknown-skill", id = id.to_string())
        })?;
        skill_ids.push(sid);
    }
    let mut mcp_ids = Vec::with_capacity(mcps.len());
    for id in mcps {
        let mid = mcp_id(id)?;
        held(ws.get_mcp(&mid), || {
            bisa_core::text!(
                "error-node-agents_api-unknown-mcp-server",
                id = id.to_string()
            )
        })?;
        mcp_ids.push(mid);
    }
    Ok((skill_ids, mcp_ids))
}

// --- agents -----------------------------------------------------------------

async fn list_agents(
    State(state): State<Shared>,
    filter: TagFilter,
) -> Result<Json<serde_json::Value>, ApiError> {
    let ws = state.engine.workspace();
    let agents = filter.apply(ws, TagEntity::Agent, ws.list_agents()?, |a| {
        a.id.to_string()
    })?;
    Ok(Json(json!({"agents": agents})))
}

async fn get_agent(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let agent = state.engine.workspace().get_agent(&agent_id(&id)?)?;
    Ok(Json(json!({"agent": agent})))
}

async fn create_agent(
    State(state): State<Shared>,
    crate::Body(body): crate::Body<NewAgentBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if body.name.trim().is_empty() || body.system_prompt.trim().is_empty() {
        return Err(bad_request(bisa_core::text!(
            "error-node-agents_api-agent-needs-name-system-prompt"
        )));
    }
    let ws = state.engine.workspace();
    let (skills, mcps) = check_refs(ws, &body.skills, &body.mcps)?;
    if let Some(photo) = &body.photo {
        crate::attachments::photo_check(ws, photo, PhotoProfile::Picture)?;
    }
    let agent = bisa_engine::directory::add_agent(
        state.engine.inner(),
        NewAgent {
            name: body.name,
            photo: body.photo,
            description: body.description,
            system_prompt: body.system_prompt,
            harness: body.harness,
            models: body.models.unwrap_or_default(),
            skills,
            mcps,
            tags: parse_tags(&body.tags)?,
            respond: parse_respond(body.respond.as_deref())?,
            decision_making: body.decision_making,
        },
    )?;
    Ok(Json(json!({"agent": agent})))
}

async fn patch_agent(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
    crate::Body(body): crate::Body<PatchAgentBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let ws = state.engine.workspace();
    let id = agent_id(&id)?;
    // Every field is read and checked before anything is written, and the
    // record is written once, through the engine: enablement is a transition
    // with consequences — membership events on every channel the agent is
    // in — and an edit the node refuses has stood nobody down.
    let mut agent = ws.get_agent(&id)?;
    if let Some(enabled) = body.enabled {
        agent.enabled = enabled;
    }
    if let Some(v) = body.name {
        agent.name = v;
    }
    if let Some(photo) = body.photo {
        if let Some(photo) = &photo {
            crate::attachments::photo_check(ws, photo, PhotoProfile::Picture)?;
        }
        agent.photo = photo;
    }
    if let Some(description) = body.description {
        agent.description = description;
    }
    if let Some(v) = body.system_prompt {
        agent.system_prompt = v;
    }
    if let Some(v) = body.harness {
        agent.harness = v;
    }
    if let Some(models) = body.models {
        agent.models = models;
    }
    if let Some(v) = &body.skills {
        agent.skills = check_refs(ws, v, &[])?.0;
    }
    if let Some(v) = &body.mcps {
        agent.mcps = check_refs(ws, &[], v)?.1;
    }
    if let Some(v) = &body.tags {
        agent.tags = parse_tags(v)?;
    }
    if let Some(v) = body.respond {
        agent.respond = parse_respond(Some(&v))?;
    }
    if let Some(v) = body.decision_making {
        agent.decision_making = v;
    }
    let agent = bisa_engine::directory::update_agent(state.engine.inner(), agent)?;
    Ok(Json(json!({"agent": agent})))
}

async fn delete_agent(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    bisa_engine::directory::remove_agent(state.engine.inner(), &agent_id(&id)?)?;
    Ok(Json(json!({"ok": true})))
}

// --- agent wiring: skills and MCP servers -----------------------------------

/// Attach a library skill. Idempotent, and the list keeps attach order — a
/// harness reads skills in order, so this is a list, not a set.
async fn attach_skill(
    State(state): State<Shared>,
    AxPath((id, skill_id)): AxPath<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let agent = bisa_engine::directory::attach_skill(
        state.engine.inner(),
        &agent_id(&id)?,
        &crate::skill_id(&skill_id)?,
    )?;
    Ok(Json(json!({"agent": agent})))
}

/// Detach a skill. Also idempotent: an agent that never carried it ends up
/// where the caller asked it to be.
async fn detach_skill(
    State(state): State<Shared>,
    AxPath((id, skill_id)): AxPath<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let agent = bisa_engine::directory::detach_skill(
        state.engine.inner(),
        &agent_id(&id)?,
        &crate::skill_id(&skill_id)?,
    )?;
    Ok(Json(json!({"agent": agent})))
}

async fn attach_mcp(
    State(state): State<Shared>,
    AxPath((id, mcp_id)): AxPath<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let agent = bisa_engine::directory::attach_mcp(
        state.engine.inner(),
        &agent_id(&id)?,
        &crate::mcp_id(&mcp_id)?,
    )?;
    Ok(Json(json!({"agent": agent})))
}

async fn detach_mcp(
    State(state): State<Shared>,
    AxPath((id, mcp_id)): AxPath<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let agent = bisa_engine::directory::detach_mcp(
        state.engine.inner(),
        &agent_id(&id)?,
        &crate::mcp_id(&mcp_id)?,
    )?;
    Ok(Json(json!({"agent": agent})))
}

/// Recall: owner-readable memory listing with `[[slug]]` links and orphans.
async fn agent_recall(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let records = state.engine.workspace().recall_list(&agent_id(&id)?)?;
    Ok(Json(json!({"agent": id, "records": records})))
}

/// Models the harness advertises for the agent editor's model-plan picker,
/// each with the effort levels it takes.
///
/// An empty list means "unknown", never "unsupported": the editor falls back
/// to free-text entry so a harness we cannot enumerate stays usable. The
/// top-level `efforts` is what the harness takes for a model it does not
/// list — what the picker offers beside an id somebody typed; empty for a
/// harness with no effort control, and for an id that names no harness.
async fn harness_models(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Json<serde_json::Value> {
    let inner = state.engine.inner();
    let ttl = state.engine.cache_settings().harness_models_ttl();
    let models = inner.catalog.models_for(&id, ttl).await;
    let efforts = bisa_engine::effort::harness_efforts(inner, &id);
    Json(json!({"harness": id, "efforts": efforts, "models": models}))
}

/// The engine's model-health ledger: which `(harness, model)` pairs are in
/// cooldown, how long is left, and how many sessions are on each right now.
///
/// **Live runtime state, not truth.** It is per-process and a restart forgets
/// it, which is the honest thing for a surface to say — an empty list means
/// "nothing to report", never "no models".
async fn models_health(State(state): State<Shared>) -> Json<serde_json::Value> {
    Json(json!({"models": state.engine.model_health()}))
}

// --- teams ------------------------------------------------------------------

/// Team members are [`Assignee`]s — the same sum type goals and projects
/// use, so "who carries this" has one vocabulary. The `team:` arm has no wire
/// form here: a team inside a team is refused at validation, and offering a
/// spelling for it would only move the refusal later.
fn parse_members(values: &[serde_json::Value]) -> Result<Vec<Assignee>, ApiError> {
    values
        .iter()
        .map(|v| {
            if let Some(h) = v.get("human").and_then(|x| x.as_str()) {
                let pk = bisa_core::PrincipalId::new(h.to_string()).map_err(|e| {
                    bad_request(bisa_core::text!(
                        "error-node-agents_api-bad-member-pubkey",
                        e = e.to_string()
                    ))
                })?;
                Ok(Assignee::Human(pk))
            } else if let Some(a) = v.get("agent").and_then(|x| x.as_str()) {
                Ok(Assignee::Agent(a.to_string()))
            } else {
                Err(bad_request(bisa_core::text!(
                    "error-node-agents_api-team-member-must-be"
                )))
            }
        })
        .collect()
}

async fn list_teams(
    State(state): State<Shared>,
    filter: TagFilter,
) -> Result<Json<serde_json::Value>, ApiError> {
    let ws = state.engine.workspace();
    let mut teams = filter.apply(ws, TagEntity::Team, ws.list_teams()?, |t| t.id.to_string())?;
    // Same membership answer as `GET /teams/{id}`, for the same reason — see
    // the comment there. A list that said five members and a detail page that
    // said six would be describing two different teams, and the reader would
    // be right to trust neither.
    for team in &mut teams {
        team.members = ws.team_participants(&team.id)?;
    }
    Ok(Json(json!({"teams": teams})))
}

async fn get_team(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let ws = state.engine.workspace();
    let id = team_id(&id)?;
    let mut team = ws.get_team(&id)?;
    // Who belongs here, which is the stored roster plus the core agent — it is
    // a member of every team and is stored in none of them, because a list you
    // can edit is a list you can empty.
    //
    // Deliberately `team_participants` and not `team_agents`. The two differ
    // by exactly this agent: `team_agents` is the pool work items route
    // across, and this one guides and delegates, so letting it win a race to
    // implement a work item would have it doing the one thing its role
    // forbids. A header, an `@`-picker and a team-wide message want the other
    // list.
    //
    // Safe to return in the `members` field a PATCH sends back: the store
    // strips the core agent on every write.
    team.members = ws.team_participants(&id)?;
    // A team detail screen needs something to show: what it is carrying.
    let owed = crate::goals::owed_goals(&state);
    let mut goals: Vec<crate::dto::GoalRow> = Vec::new();
    for g in ws
        .goals_for_assignee(&bisa_core::Assignee::Team(id.to_string()))
        .unwrap_or_default()
    {
        goals.push(crate::goals::goal_row(&state, &owed, &g)?);
    }
    Ok(Json(json!({"team": team, "goals": goals})))
}

async fn create_team(
    State(state): State<Shared>,
    crate::Body(body): crate::Body<TeamBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if body.name.trim().is_empty() {
        return Err(bad_request(bisa_core::text!(
            "error-node-agents_api-team-needs-name"
        )));
    }
    let members = parse_members(&body.members)?;
    if let Some(photo) = &body.photo {
        crate::attachments::photo_check(state.engine.workspace(), photo, PhotoProfile::Picture)?;
    }
    let team = bisa_engine::directory::create_team(
        state.engine.inner(),
        &body.name,
        body.purpose.as_deref(),
        members,
        parse_tags(&body.tags)?,
    )?;
    // The picture rides the record once it exists — one creation, one photo.
    let team = match body.photo {
        Some(photo) => bisa_engine::directory::update_team(
            state.engine.inner(),
            Team {
                photo: Some(photo),
                ..team
            },
        )?,
        None => team,
    };
    Ok(Json(json!({"team": team})))
}

async fn patch_team(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
    crate::Body(body): crate::Body<TeamPatch>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let ws = state.engine.workspace();
    let id = team_id(&id)?;
    let existing = ws.get_team(&id)?;
    // A PATCH is partial: omitted fields keep their current value, so a
    // caller editing only the roster does not have to resend the name.
    let members = match &body.members {
        Some(values) => parse_members(values)?,
        None => existing.members.clone(),
    };
    let tags = match &body.tags {
        Some(raw) => parse_tags(raw)?,
        None => existing.tags.clone(),
    };
    let photo = match body.photo {
        Some(photo) => {
            if let Some(photo) = &photo {
                crate::attachments::photo_check(ws, photo, PhotoProfile::Picture)?;
            }
            photo
        }
        None => existing.photo.clone(),
    };
    // One write, through the engine, once every field was read: it is what
    // announces the membership change in every channel the team rosters,
    // and an edit the node refuses has stood nobody down.
    let team = bisa_engine::directory::update_team(
        state.engine.inner(),
        Team {
            id: existing.id,
            name: body.name.unwrap_or(existing.name),
            purpose: body.purpose.unwrap_or(existing.purpose),
            enabled: body.enabled.unwrap_or(existing.enabled),
            photo,
            members,
            tags,
            ..existing
        },
    )?;
    Ok(Json(json!({"team": team})))
}

async fn delete_team(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    bisa_engine::directory::remove_team(state.engine.inner(), &team_id(&id)?)?;
    Ok(Json(json!({"ok": true})))
}

// --- runtime sessions (the old /agents roster, renamed) ---------------------

/// Every session the engine drives, as presence folds it (`presence.rs`):
/// the one status that leaves the engine, newest state change first. Finished
/// sessions stay for the retention window, then leave with `session_gone`.
async fn sessions(State(state): State<Shared>) -> Json<SessionsResponse> {
    let ttl = state.engine.cache_settings().presence_ttl();
    Json(SessionsResponse {
        sessions: state.engine.inner().presence.snapshot_cached(ttl),
    })
}

async fn session(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<Json<bisa_engine::SessionPresence>, ApiError> {
    let id = bisa_engine::LiveRunId::from_str(&id).map_err(|_| {
        bad_request(bisa_core::text!(
            "error-node-agents_api-not-session-id",
            id = format!("{id:?}")
        ))
    })?;
    state
        .engine
        .inner()
        .presence
        .get(id)
        .map(Json)
        .ok_or_else(|| {
            not_found(bisa_core::text!(
                "error-node-agents_api-no-session",
                id = id.to_string()
            ))
        })
}

async fn abort_session(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = bisa_engine::LiveRunId::from_str(&id).map_err(|_| {
        bad_request(bisa_core::text!(
            "error-node-agents_api-not-session-id",
            id = format!("{id:?}")
        ))
    })?;
    // The engine's one door: whatever drives the session is what stops it,
    // and the answer comes once its process is gone — or was terminated at
    // the deadline — so the row never reads aborted beside a harness still
    // at work, and the person is told which it was.
    let ended = bisa_engine::sessions::stop_one_settled(state.engine.inner(), id)
        .await
        .map_err(|_| {
            not_found(bisa_core::text!(
                "error-node-agents_api-no-session",
                id = id.to_string()
            ))
        })?;
    Ok(Json(json!({"ok": true, "ended": ended})))
}

// --- interactive sessions: a harness in a desktop terminal ------------------

/// The desk's refusal as the node answers it: the status the refusal means,
/// and the engine's own message — never its English sentence carried as an
/// argument, which a reader in another language would be shown as it is.
fn interactive_error(e: bisa_engine::interactive::InteractiveError) -> ApiError {
    use bisa_engine::interactive::InteractiveError as E;
    let status = match &e {
        E::UnknownHarness(_) | E::NotInteractive(_) => StatusCode::BAD_REQUEST,
        E::UnknownSession(_) => StatusCode::NOT_FOUND,
        E::BadSecret => StatusCode::UNAUTHORIZED,
        E::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
        E::Store(_) => {
            return match e {
                E::Store(store) => store.into(),
                other => ApiError::text(StatusCode::INTERNAL_SERVER_ERROR, other.text()),
            }
        }
    };
    ApiError::text(status, e.text())
}

fn session_id(id: &str) -> Result<bisa_engine::LiveRunId, ApiError> {
    bisa_engine::LiveRunId::from_str(id).map_err(|_| {
        bad_request(bisa_core::text!(
            "error-node-agents_api-not-session-id",
            id = format!("{id:?}")
        ))
    })
}

/// The secret an interactive session's hook presents, in the same place the
/// control-plane token would go.
fn session_secret(headers: &HeaderMap, uri: &Uri) -> Result<String, ApiError> {
    crate::auth::presented_in(headers, uri).ok_or_else(|| {
        ApiError::text(
            StatusCode::UNAUTHORIZED,
            bisa_core::text!("error-node-agents_api-session-secret-required"),
        )
    })
}

/// Register a harness a person is opening in a terminal and answer the plan
/// that makes it report: the session, its secret, the environment and the
/// arguments. A harness with nothing to report answers no session.
async fn open_terminal_session(
    State(state): State<Shared>,
    crate::Body(body): crate::Body<OpenInteractiveBody>,
) -> Result<Json<InteractiveOpened>, ApiError> {
    let scope = bisa_store::FileScope::from_str(&body.scope).map_err(|_| {
        bad_request(bisa_core::text!(
            "error-node-agents_api-unknown-scope-use-goal-workstream-work-item",
            a0 = format!("{:?}", body.scope)
        ))
    })?;
    let inner = state.engine.inner();
    let opened = inner
        .interactive
        .open(
            inner,
            bisa_engine::interactive::OpenInteractive {
                scope,
                id: body.id,
                harness: body.harness,
            },
        )
        .map_err(interactive_error)?;
    Ok(Json(match opened {
        None => InteractiveOpened::default(),
        Some(o) => InteractiveOpened {
            session: Some(o.session.to_string()),
            env: o.env,
            env_remove: o.env_remove,
            args: o.args,
            intercept_approval_notifications: o.intercept_approval_notifications,
        },
    }))
}

/// Events a hook inside the session reported, folded like any harness's.
async fn report_session(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
    headers: HeaderMap,
    uri: Uri,
    crate::Body(body): crate::Body<ReportBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = session_id(&id)?;
    let secret = session_secret(&headers, &uri)?;
    let inner = state.engine.inner();
    inner
        .interactive
        .report(inner, id, &secret, &body.events)
        .map_err(interactive_error)?;
    Ok(Json(json!({"ok": true})))
}

/// The guard hook inside a terminal-hosted harness asks before a tool runs:
/// the verdict the hook prints back — `deny` with the reason, `ask`, `allow`
/// with a restored input — or nothing, when the guard has no opinion and the
/// harness's own prompt stands. Bearer = the session's secret.
async fn guard_session(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
    headers: HeaderMap,
    uri: Uri,
    crate::Body(body): crate::Body<GuardHookBody>,
) -> Result<Json<bisa_engine::security::GuardReply>, ApiError> {
    let id = session_id(&id)?;
    let secret = session_secret(&headers, &uri)?;
    let inner = state.engine.inner();
    let reply = inner
        .interactive
        .guard(inner, id, &secret, &body.payload)
        .await
        .map_err(interactive_error)?;
    Ok(Json(reply))
}

/// The process behind the session ended by itself: the row moves to what the
/// exit means and is held beside its tab.
async fn exit_session(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
    headers: HeaderMap,
    uri: Uri,
    crate::Body(body): crate::Body<ExitBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = session_id(&id)?;
    let secret = session_secret(&headers, &uri)?;
    let inner = state.engine.inner();
    let report = bisa_engine::interactive::ExitReport {
        code: body.code,
        signal: body.signal,
    };
    inner
        .interactive
        .exited(inner, id, &secret, &report)
        .map_err(interactive_error)?;
    Ok(Json(json!({"ok": true})))
}

/// The terminal tab behind the session is gone: the row leaves the roster now.
async fn close_session(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = session_id(&id)?;
    let secret = session_secret(&headers, &uri)?;
    let inner = state.engine.inner();
    inner
        .interactive
        .close(inner, id, &secret)
        .map_err(interactive_error)?;
    Ok(Json(json!({"ok": true})))
}

/// The person answered the harness's dialog in the terminal tab: every wait
/// of the session is over and the row goes back to what it was doing.
async fn answered_session(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = session_id(&id)?;
    let secret = session_secret(&headers, &uri)?;
    let inner = state.engine.inner();
    inner
        .interactive
        .answered(inner, id, &secret)
        .map_err(interactive_error)?;
    Ok(Json(json!({"ok": true})))
}

/// The routes this module mounts, for `bisa-node --bin api-docs` and the
/// route test. Kept beside `routes()` so a route added here is added here.
pub const ROUTES: &[RouteDoc] = &[
    RouteDoc { method: "GET", path: "/agents", summary: "Every agent definition, the general agent first." },
    RouteDoc { method: "POST", path: "/agents", summary: "Define an agent: `{name, system_prompt, harness, description?, models?, skills?, mcps?, tags?, respond?, decision_making?, photo?}` — `decision_making` lets the Decision-Making Agent stand in at the decision points this agent reaches; `photo` an attachment this machine holds, a picture within `MAX_PHOTO_BYTES` (ide/14 §Photos). A key the body does not know is a 400, and so is a name whose id is the Decision-Making Agent's." },
    RouteDoc { method: "GET", path: "/agents/{id}", summary: "One agent definition." },
    RouteDoc { method: "PATCH", path: "/agents/{id}", summary: "Edit an agent: what the body leaves out is kept, and `photo: null` or `description: null` takes that away. `enabled` stands the agent down or up, said in every channel it is in. The record is written once, after every field was read: an edit that is refused changed nothing and stood nobody down. The General Agent and the Workflow Agent accept only `harness`, `models` and `decision_making`. A key the body does not know is a 400; a skill or a server nobody has is a 400, and one whose record cannot be read is the store's own answer, naming its file." },
    RouteDoc { method: "DELETE", path: "/agents/{id}", summary: "Delete an agent, or 409 while a team or channel still names it; the general agent is permanent." },
    RouteDoc { method: "GET", path: "/agents/{id}/recall", summary: "The agent's memory: records with `[[slug]]` links and orphans." },
    RouteDoc { method: "POST", path: "/agents/{id}/skills/{skill_id}", summary: "Attach a skill. Idempotent; the list keeps attach order." },
    RouteDoc { method: "DELETE", path: "/agents/{id}/skills/{skill_id}", summary: "Detach a skill. Idempotent." },
    RouteDoc { method: "POST", path: "/agents/{id}/mcps/{mcp_id}", summary: "Attach an MCP server. Idempotent." },
    RouteDoc { method: "DELETE", path: "/agents/{id}/mcps/{mcp_id}", summary: "Detach an MCP server. Idempotent." },
    RouteDoc { method: "GET", path: "/harnesses/{id}/models", summary: "The models one harness advertises and the effort levels each takes, for the model-plan picker." },
    RouteDoc { method: "GET", path: "/models/health", summary: "The engine's model-health ledger: cooldowns and failures per `(harness, model)`." },
    RouteDoc { method: "GET", path: "/teams", summary: "Every team." },
    RouteDoc { method: "POST", path: "/teams", summary: "Create a team: `{name, purpose?, members?, tags?, photo?}`; members in assignee wire form, `photo` a picture this machine holds." },
    RouteDoc { method: "GET", path: "/teams/{id}", summary: "One team, with the goals it carries." },
    RouteDoc { method: "PATCH", path: "/teams/{id}", summary: "Edit a team: what the body leaves out is kept, and `photo: null` or `purpose: null` takes that away; `enabled: false` withdraws it from every channel it is on, and a message that names it then reaches nobody. Written once, after every field was read: an edit that is refused changed nothing." },
    RouteDoc { method: "DELETE", path: "/teams/{id}", summary: "Delete a team, or 409 while a goal or channel still names it." },
    RouteDoc { method: "GET", path: "/sessions", summary: "Every session — its `kind` one of `worker` (a step's work item), `guided` (the Workflow Agent's design wake), `conversation` (an agent's turn in a conversation, a channel or a goal's thread, naming its `conversation` when it is one) and `terminal` (a harness a person opened in a desktop terminal) — with its state (starting · idle · thinking · running · waiting · done · aborted · failed · parked), harness, model and the effort it runs at, agent, sub-agents and cost; a finished engine session stays a minute, a finished terminal harness stays as long as its tab." },
    RouteDoc { method: "GET", path: "/sessions/{id}", summary: "One session's presence; 404 once it has left the roster." },
    RouteDoc { method: "POST", path: "/sessions/{id}/abort", summary: "Stop one session for good, whatever drives it: a worker's harness is aborted and its work item settles as failed; a conversation turn's session is let go of, and the next message is answered by a fresh one; the Workflow Agent's design wake is aborted and the goal says its design failed because it was stopped; a terminal session's row reads aborted and the desktop closes its terminal, which forgets the row. The row reads `aborted` at once and stays so. Stopping a session that is over changes nothing (200); 400 for an id that is none, 404 for one the roster does not have. Answers once the session's process is gone — or was terminated at the deadline — with `ended: {sessions, terminated, still_live, children}`." },
    RouteDoc { method: "POST", path: "/sessions/terminal", summary: "Register a harness a person is opening in a desktop terminal: `{scope, id, harness}` → the session (`kind: terminal`), and the environment (its secret among it) and arguments that make it report; no session when the harness cannot. A session stands where a terminal can open: a scope nobody knows, an id that is no id and a harness with no interactive form are a 400, an id of nothing the workspace has a 404 — and nothing is registered or written." },
    RouteDoc { method: "POST", path: "/sessions/{id}/report", summary: "Events a hook inside a terminal session reported: `{events}`. Bearer = the session's secret, never the control-plane token. What a harness says about its own tool calls is redacted before it reaches a roster row." },
    RouteDoc { method: "POST", path: "/sessions/{id}/guard", summary: "The guard hook inside a terminal-hosted harness asks before a tool runs: `{payload}` (the harness's `PreToolUse` payload) → `{decision?: allow | deny | ask, reason?, updated_input?}`; an empty answer means the guard has no opinion and the harness's own prompt stands. Bearer = the session's secret." },
    RouteDoc { method: "POST", path: "/sessions/{id}/exit", summary: "The process behind a terminal session ended by itself: `{code?, signal?}` → done on 0, failed otherwise, the row held beside its tab. Bearer = the session's secret." },
    RouteDoc { method: "POST", path: "/sessions/{id}/close", summary: "The terminal tab behind a terminal session is gone: the row leaves the roster at once. Bearer = the session's secret." },
    RouteDoc { method: "POST", path: "/sessions/{id}/answered", summary: "The person answered the harness's dialog in its terminal tab — approved, declined or escaped it; no hook says so, the tab does. Every wait of the session and of its sub-agents is over and the row goes back to what it was doing: *running <tool>* for a call it already announced, else *thinking*; the harness's next word corrects it whichever way the person answered. Bearer = the session's secret." },
];
