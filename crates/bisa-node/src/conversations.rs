//! Conversations: the routes over a saved exchange with agents
//! ([`bisa_core::Conversation`]).
//!
//! A conversation is addressed by its own id, never by the thing it is about:
//! `GET /conversations?origin=&id=` lists the ones about something, `POST
//! /conversations` starts one with an origin, and its messages are its own
//! stream (`/conversations/{id}/messages`, the same body and the same chip
//! bound as a channel's). `GET /conversations/{id}/live` is the turns in
//! flight — each agent's words and thinking so far — for a timeline that
//! joins mid-turn; the frames themselves ride the bus (`agent_streamed`).
//!
//! The record moves through [`bisa_engine::conversations`], so every change
//! stops the turns it should and reaches the bus.

use crate::dto::{
    ConversationView, LiveTurnView, LiveTurnsResponse, NewConversationBody, NewMessageBody,
    PatchConversationBody,
};
use crate::route_docs::RouteDoc;
use crate::Query;
use crate::{agent_id, bad_request, ApiError, Shared};
use axum::extract::{Path as AxPath, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use bisa_core::{ConversationId, ConversationOrigin};
use bisa_engine::conversations as eng;
use bisa_store::ConversationFilter;
use serde::Deserialize;
use serde_json::json;
use std::str::FromStr;

pub(crate) fn routes() -> Router<Shared> {
    Router::new()
        .route("/conversations", get(list).post(create))
        .route(
            "/conversations/{id}",
            get(get_one).patch(patch).delete(delete),
        )
        .route(
            "/conversations/{id}/messages",
            get(messages).post(post_message),
        )
        .route("/conversations/{id}/live", get(live))
        .route("/conversations/{id}/plan/build", post(build_plan))
}

fn parse_id(s: &str) -> Result<ConversationId, ApiError> {
    ConversationId::from_str(s).map_err(|_| {
        bad_request(bisa_core::text!(
            "error-node-changes-not-conversation-id",
            s = format!("{s:?}")
        ))
    })
}

/// How many a list answers unasked, and the most it will.
const DEFAULT_LIMIT: usize = 50;
const MAX_LIMIT: usize = 200;

#[derive(Deserialize)]
struct ListQuery {
    /// One of the origin kinds: `node` · `workspace` · `goal` · `workflow` ·
    /// `project` · `workstream` · `drawing` · `note`.
    #[serde(default)]
    origin: Option<String>,
    /// The record the origin names, with `origin`.
    #[serde(default)]
    id: Option<String>,
    /// Every conversation standing in one project — its own and its
    /// checkouts' — what the Project IDE lists. Instead of `origin`, never
    /// beside it.
    #[serde(default)]
    project: Option<String>,
    /// Those an agent spoke in or was addressed in.
    #[serde(default)]
    agent: Option<String>,
    /// Words to find in the messages or the titles.
    #[serde(default)]
    q: Option<String>,
    /// `false` for the live ones, `true` for the archived; absent for both.
    #[serde(default)]
    archived: Option<bool>,
    /// Page: only conversations that moved before this moment — and before
    /// the row `before_id` names, when it is given, so two that moved within
    /// one second are never split by a page.
    #[serde(default)]
    before: Option<u64>,
    #[serde(default)]
    before_id: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

async fn list(
    State(state): State<Shared>,
    Query(q): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if let Some(kind) = q.origin.as_deref() {
        if !ConversationOrigin::KINDS.contains(&kind) {
            return Err(bad_request(bisa_core::text!(
                "error-node-conversations-unknown-origin-one",
                kind = format!("{kind:?}"),
                a0 = (ConversationOrigin::KINDS.join(", ")).to_string()
            )));
        }
        if ConversationOrigin::kind_takes_id(kind) != q.id.is_some() {
            return Err(bad_request(bisa_core::text!(
                "error-node-conversations-origin-id",
                kind = format!("{kind:?}"),
                given = if q.id.is_some() { "yes" } else { "no" }
            )));
        }
    } else if q.id.is_some() {
        return Err(bad_request(bisa_core::text!(
            "error-node-conversations-id-needs-origin"
        )));
    }
    if q.project.is_some() && q.origin.is_some() {
        return Err(bad_request(bisa_core::text!(
            "error-node-conversations-narrow-project-origin-not-both"
        )));
    }
    let project = q
        .project
        .as_deref()
        .map(crate::projects::parse_project_id)
        .transpose()?;
    let filter = ConversationFilter {
        origin_kind: q.origin,
        origin_id: q.id,
        project,
        agent: q.agent.as_deref().map(agent_id).transpose()?,
        archived: q.archived,
        query: q.q,
        before: q.before.map(|at| bisa_store::PageBefore {
            at,
            id: q.before_id.clone(),
        }),
        limit: q.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT),
    };
    let rows = eng::list(state.engine.inner(), &filter)?;
    Ok(Json(json!({
        "conversations": rows.into_iter().map(ConversationView::from).collect::<Vec<_>>(),
    })))
}

async fn create(
    State(state): State<Shared>,
    crate::Body(body): crate::Body<NewConversationBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let inner = state.engine.inner();
    let mode = body
        .mode
        .unwrap_or_else(|| eng::default_mode(inner, &body.origin));
    let made = eng::create(
        inner,
        bisa_store::NewConversation {
            origin: body.origin,
            title: body.title,
            mode,
        },
    )?;
    let view = ConversationView::from(eng::get(state.engine.inner(), made.id)?);
    Ok((StatusCode::CREATED, Json(json!({"conversation": view}))))
}

async fn get_one(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_id(&id)?;
    let view = ConversationView::from(eng::get(state.engine.inner(), id)?);
    Ok(Json(json!({"conversation": view})))
}

/// `title` is tri-state: absent keeps it, `null` takes it away, a value
/// sets it. `archived` puts the conversation away or takes it back out.
async fn patch(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
    crate::Body(body): crate::Body<PatchConversationBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_id(&id)?;
    let inner = state.engine.inner();
    if let Some(title) = body.title {
        eng::rename(inner, id, title.as_deref())?;
    }
    if let Some(archived) = body.archived {
        eng::set_archived(inner, id, archived).await?;
    }
    if let Some(mode) = body.mode {
        eng::set_mode(inner, id, mode)?;
    }
    let view = ConversationView::from(eng::get(inner, id)?);
    Ok(Json(json!({"conversation": view})))
}

async fn delete(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<StatusCode, ApiError> {
    let id = parse_id(&id)?;
    eng::delete(state.engine.inner(), id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn messages(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
    Query(q): Query<crate::conversation::PageQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_id(&id)?;
    eng::record(state.engine.inner(), id)?;
    crate::conversation::scope_messages(&state, &id.to_string(), &q)
}

async fn post_message(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
    crate::Body(body): crate::Body<NewMessageBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_id(&id)?;
    let conversation = eng::record(state.engine.inner(), id)?;
    if conversation.archived {
        return Err(ApiError::text(
            StatusCode::CONFLICT,
            bisa_core::text!(
                "error-node-conversations-conversation-archived-take-back-out-continue"
            ),
        ));
    }
    crate::conversation::scope_post(&state, &id.to_string(), body)
}

/// *Build this plan*: the mode goes back to what it was before the plan —
/// the engine's rule, so no screen remembers a mode.
async fn build_plan(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_id(&id)?;
    let view = ConversationView::from(eng::get(
        state.engine.inner(),
        eng::build_plan(state.engine.inner(), id)?.id,
    )?);
    Ok(Json(json!({"conversation": view})))
}

/// The turns in flight: each agent's words and thinking so far, oldest turn
/// first; `{turns: []}` when nothing runs. 404 when unknown.
async fn live(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<Json<LiveTurnsResponse>, ApiError> {
    let id = parse_id(&id)?;
    let turns = eng::live_turns(state.engine.inner(), id)?
        .into_iter()
        .map(LiveTurnView::from)
        .collect();
    Ok(Json(LiveTurnsResponse { turns }))
}

/// The routes this module mounts, for `bisa-node --bin api-docs` and the
/// route test. Kept beside `routes()` so a route added here is added here.
pub const ROUTES: &[RouteDoc] = &[
    RouteDoc {
        method: "GET",
        path: "/conversations",
        summary: "Conversations, the most recently moved first: `?origin=<node|workspace|goal|workflow|project|workstream|drawing|note>&id=` narrows to what they are about, `?project=<id>` to every one standing in a project (its own and its checkouts'; never beside `origin`), `?agent=` to those an agent spoke in or was addressed in, `?q=` finds words in their messages or titles, `?archived=` one side, `?before=&limit=` pages. Each row carries its origin, title, first line, counts and the agents in it.",
    },
    RouteDoc {
        method: "POST",
        path: "/conversations",
        summary: "Start a conversation: `{origin: {kind, id?, project?}, title?, mode?}` → 201 with the conversation. The origin is checked: a goal, workflow, project, workstream, drawing or note that is not here is refused. `mode` absent is `agents.conversation.mode` where the conversation stands.",
    },
    RouteDoc {
        method: "GET",
        path: "/conversations/{id}",
        summary: "One conversation with its facts; 404 when unknown.",
    },
    RouteDoc {
        method: "PATCH",
        path: "/conversations/{id}",
        summary: "Edit a conversation: `{title?, archived?, mode?}` — `title` is tri-state (absent keeps, `null` takes it away, a value sets it); `archived: true` stops the turn that may be running and puts it away, `false` takes it back out; `mode` is `manual`, `auto` or `plan`, for a project's or a workstream's conversation only, and holds from the next call a live turn makes.",
    },
    RouteDoc {
        method: "DELETE",
        path: "/conversations/{id}",
        summary: "Delete a conversation: its turns stopped, then the record, its messages and its rows; 204.",
    },
    RouteDoc {
        method: "GET",
        path: "/conversations/{id}/messages",
        summary: "The conversation's messages (`?before=&before_id=&limit=`), with reactions; an agent's reply carries the `thinking` that led to it beside its words.",
    },
    RouteDoc {
        method: "POST",
        path: "/conversations/{id}/messages",
        summary: "Post into the conversation: `{content, reply_to?, mentions?, attachments?, artifacts?, context?}`. `context` is the chips the person attached; nothing else is injected. 409 on an archived conversation.",
    },
    RouteDoc {
        method: "POST",
        path: "/conversations/{id}/plan/build",
        summary: "Build this plan: a conversation in `plan` goes back to the mode it was in before the plan — the record remembers which — or to `manual` when it began in one; answers the conversation. 409 when it is not in `plan`, 400 for a conversation that is about no checkout, 404 when unknown. No body.",
    },
    RouteDoc {
        method: "GET",
        path: "/conversations/{id}/live",
        summary: "The turns in flight: `{turns: [{agent, text, thinking, since}]}` — each answering agent's words and thinking so far, oldest turn first, empty when nothing runs. A timeline that joins mid-turn reads this once, then appends the bus's `agent_streamed` frames; `agent_replied` says the message landed.",
    },
];
