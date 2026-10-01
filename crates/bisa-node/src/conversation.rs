//! The Studio's conversation routes: Channels (standing conversations), Messages
//! (DM channels with a restricted audience), reactions/retractions, read
//! markers.

use crate::dto::*;
use crate::route_docs::RouteDoc;
use crate::tags::{parse_tags, TagFilter};
use crate::Query;
use crate::{agent_id, bad_request, channel_id, conflict, team_id, ApiError, Shared};
use axum::extract::{Path as AxPath, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use bisa_core::tags::TagEntity;
use bisa_core::Localize as _;
use bisa_core::{AgentId, ChannelKind, DeletableChannel, PrincipalId, RosterPolicy, TeamId};
use serde::Deserialize;
use serde_json::json;

pub(crate) fn routes() -> Router<Shared> {
    Router::new()
        .route("/channels", get(list_channels).post(create_channel))
        .route(
            "/channels/{id}",
            get(get_channel).patch(patch_channel).delete(delete_channel),
        )
        .route(
            "/channels/{id}/messages",
            get(channel_messages).post(channel_post),
        )
        .route("/dms", get(list_dms).post(open_dm))
        .route("/messages/{id}", get(get_message))
        .route("/messages/{id}/retract", post(retract))
        .route("/messages/{id}/react", post(react))
        .route("/reactions/{id}/retract", post(retract))
        .route("/read", post(mark_read))
        .route("/unread", post(mark_unread))
}

/// Pubkeys, and only pubkeys — an audience and a DM's participant list are
/// sets of keys, not names. Mentions go through
/// [`bisa_store::Workspace::resolve_mentions`] instead, because a mention
/// may also be an agent id or a channel handle.
/// A roster from the wire: agent ids, team ids and people's pubkeys, every
/// one checked as an id here and as a real record by the store.
fn roster_of(
    agents: &[String],
    teams: &[String],
    humans: &[String],
) -> Result<RosterPolicy, ApiError> {
    let agents: Vec<AgentId> = agents
        .iter()
        .map(|a| agent_id(a))
        .collect::<Result<_, _>>()?;
    let teams: Vec<TeamId> = teams.iter().map(|t| team_id(t)).collect::<Result<_, _>>()?;
    let humans = parse_principals(humans)?;
    Ok(RosterPolicy::Listed {
        agents,
        teams,
        humans,
    })
}

fn parse_principals(hexes: &[String]) -> Result<Vec<PrincipalId>, ApiError> {
    hexes
        .iter()
        .map(|h| {
            PrincipalId::new(h.clone()).map_err(|e| {
                bad_request(bisa_core::text!(
                    "error-node-conversation-bad-pubkey",
                    h = format!("{h:?}"),
                    e = e.to_string()
                ))
            })
        })
        .collect()
}

// --- shared scope helpers (goal threads reuse these) ----------------------

pub(crate) fn scope_messages(
    state: &Shared,
    scope: &str,
    page: &PageQuery,
) -> Result<Json<serde_json::Value>, ApiError> {
    let ws = state.engine.workspace();
    let messages = ws.messages(scope, page.cursor(), page.size())?;
    let reactions = ws.list_reactions(scope)?;
    Ok(Json(json!({
        "scope": scope,
        "messages": messages,
        "reactions": reactions,
    })))
}

/// Post into a scope, addressing whoever the mention tokens name.
///
/// **The mentions are resolved by the store**, not here: a token may be a
/// pubkey, an agent definition id, or the scope's own id — the channel handle,
/// which expands to that channel's roster. Doing it in one place is what makes
/// the handle work identically from the node, the CLI and the MCP tool, and an
/// unknown token comes back as a refusal rather than a silent drop.
///
/// **No mentions means nobody is addressed**, even in a channel with a full
/// roster. A roster is a directory, not a subscription: five rostered agents
/// must never mean five harness sessions per message. The enforcement lives in
/// `bisa-engine/src/conversation.rs::dispatch`, which wakes only the
/// agents a message `p`-tags; this route's part of it is refusing to invent
/// `p` tags nobody asked for.
pub(crate) fn scope_post(
    state: &Shared,
    scope: &str,
    body: NewMessageBody,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Every route that posts a message lands here, and every one of them is a
    // person typing: the desktop app, and anything else driving the HTTP API on
    // their behalf. The platform's own posts do not come through here.
    let id = bisa_engine::messaging::post_as_person(
        state.engine.inner(),
        scope,
        bisa_engine::messaging::PersonPost {
            text: body.content,
            context: body.context,
            reply_to: body.reply_to,
            mentions: body.mentions,
            attachments: body.attachments,
            artifacts: body.artifacts,
        },
    )?;
    Ok(Json(json!({"id": id})))
}

/// `GET /messages/{id}` — one message wherever it lives, with its files and
/// artifacts and whether each is on this machine. The door a viewer opens a
/// single artifact by, without paging its conversation.
async fn get_message(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let row = state.engine.workspace().get_message(&id)?.ok_or_else(|| {
        crate::not_found(bisa_core::text!(
            "error-node-conversation-no-message-with-id"
        ))
    })?;
    Ok(Json(json!({"message": row})))
}

// --- channels ----------------------------------------------------------------

/// Standing channels only. A DM is a conversation with an immutable audience,
/// not a room anybody joins, and it has its own route — listing it here made
/// every consumer of this one (sidebar, channels index, a message event's
/// conversation picker, command palette) show each DM twice, once as itself
/// and once as a channel.
async fn list_channels(
    State(state): State<Shared>,
    filter: TagFilter,
) -> Result<Json<serde_json::Value>, ApiError> {
    let ws = state.engine.workspace();
    let standing = ws.list_channels_of_kind(ChannelKind::Standing)?;
    let channels = filter.apply(ws, TagEntity::Channel, standing, |c| c.id.to_string())?;
    let unread: std::collections::HashMap<String, u64> = ws.unread_counts()?.into_iter().collect();
    let latest = ws.latest_messages()?;
    let rows: Vec<_> = channels
        .into_iter()
        .map(|c| {
            let n = unread.get(c.id.as_str()).copied().unwrap_or(0);
            // What a list says of the room: its latest live post, or nothing yet.
            let last = latest.get(c.id.as_str()).map(crate::inbox::preview_of);
            json!({"channel": c, "unread_count": n, "latest": last})
        })
        .collect();
    Ok(Json(json!({"channels": rows})))
}

async fn create_channel(
    State(state): State<Shared>,
    crate::Body(body): crate::Body<NewChannelBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if body.name.trim().is_empty() {
        return Err(bad_request(bisa_core::text!(
            "error-node-conversation-channel-name-must-not-be-empty"
        )));
    }
    let roster = roster_of(&body.agents, &body.teams, &body.humans)?;
    let channel = bisa_engine::messaging::create_channel(
        state.engine.inner(),
        &body.name,
        body.topic.as_deref(),
        roster,
        parse_tags(&body.tags)?,
    )?;
    Ok(Json(json!({"channel": channel})))
}

async fn get_channel(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let ws = state.engine.workspace();
    let id = channel_id(&id)?;
    let channel = ws.get_channel(&id)?;
    // The roster's pubkeys alongside it: that is what `@<channel>` expands to,
    // and a header that lists the room needs the keys, not just the ids. The
    // members are derived — roster ∩ enablement — and listed as such.
    let roster: Vec<String> = ws
        .channel_roster_pubkeys(&id)?
        .into_iter()
        .map(|p| p.to_string())
        .collect();
    let members = ws.channel_members(&id)?;
    Ok(Json(
        json!({"channel": channel, "roster_pubkeys": roster, "members": members}),
    ))
}

/// Edit a standing channel's topic, roster and tags. Omitted fields keep their
/// current value.
///
/// A DM is refused by the store — it has no roster and no topic, and its
/// audience is what past messages were encrypted to — and that refusal
/// arrives as a 400 rather than a server fault, because it is the caller's to
/// act on.
async fn patch_channel(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
    crate::Body(body): crate::Body<PatchChannelBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let ws = state.engine.workspace();
    let id = channel_id(&id)?;
    let stored = ws.get_channel(&id)?;
    // A roster edit replaces the roster; either list omitted keeps what is
    // stored for that half.
    let roster = match (body.agents, body.teams, body.humans) {
        (None, None, None) => stored.roster,
        (agents, teams, humans) => {
            let (kept_agents, kept_teams, kept_humans) = match &stored.roster {
                RosterPolicy::Listed {
                    agents,
                    teams,
                    humans,
                } => (
                    agents.iter().map(|a| a.to_string()).collect::<Vec<_>>(),
                    teams.iter().map(|t| t.to_string()).collect::<Vec<_>>(),
                    humans
                        .iter()
                        .map(|h| h.as_hex().to_string())
                        .collect::<Vec<_>>(),
                ),
                RosterPolicy::Everyone => (vec![], vec![], vec![]),
            };
            roster_of(
                &agents.unwrap_or(kept_agents),
                &teams.unwrap_or(kept_teams),
                &humans.unwrap_or(kept_humans),
            )?
        }
    };
    let tags = match &body.tags {
        Some(raw) => parse_tags(raw)?,
        None => stored.tags,
    };
    let channel = bisa_engine::messaging::update_channel(
        state.engine.inner(),
        &id,
        body.topic.or(stored.topic).as_deref(),
        roster,
        tags,
    )?;
    Ok(Json(json!({"channel": channel})))
}

/// Delete a channel. `general` is permanent: the type refuses to name it as
/// deletable, and that refusal is a 409 — the record is fine, the request is
/// not. A channel something still points at is refused by the store.
async fn delete_channel(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let ws = state.engine.workspace();
    let id = channel_id(&id)?;
    let channel = ws.get_channel(&id)?;
    let deletable = DeletableChannel::new(channel).map_err(|e| conflict(e.text()))?;
    bisa_engine::messaging::delete_channel(state.engine.inner(), deletable).await?;
    Ok(Json(json!({"ok": true, "channel": id})))
}

#[derive(Deserialize)]
pub(crate) struct PageQuery {
    /// The moment of the oldest message shown, in seconds.
    #[serde(default)]
    pub(crate) before: Option<u64>,
    /// That message's id: with it the page continues from exactly that
    /// row, and two messages posted within one second are never split.
    #[serde(default)]
    pub(crate) before_id: Option<String>,
    #[serde(default)]
    pub(crate) limit: Option<usize>,
}

/// One page's size: `limit` within `[1, 200]`, 50 unless said.
pub(crate) const DEFAULT_PAGE: usize = 50;
pub(crate) const MAX_PAGE: usize = 200;

impl PageQuery {
    pub(crate) fn cursor(&self) -> Option<bisa_store::PageBefore> {
        self.before.map(|at| bisa_store::PageBefore {
            at,
            id: self.before_id.clone(),
        })
    }

    pub(crate) fn size(&self) -> usize {
        self.limit.unwrap_or(DEFAULT_PAGE).clamp(1, MAX_PAGE)
    }
}

/// A channel's page. The channel first: a deleted channel's log stays on
/// disk as history, and its id answers 404 here, not that history.
async fn channel_messages(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
    Query(q): Query<PageQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state.engine.workspace().get_channel(&channel_id(&id)?)?;
    scope_messages(&state, &id, &q)
}

async fn channel_post(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
    crate::Body(body): crate::Body<NewMessageBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    scope_post(&state, &id, body)
}

// --- DMs --------------------------------------------------------------------

async fn list_dms(State(state): State<Shared>) -> Result<Json<serde_json::Value>, ApiError> {
    let ws = state.engine.workspace();
    let unread: std::collections::HashMap<String, u64> = ws.unread_counts()?.into_iter().collect();
    let latest = ws.latest_messages()?;
    let rows: Vec<_> = ws
        .list_channels_of_kind(ChannelKind::Direct)?
        .into_iter()
        .map(|c| {
            let n = unread.get(c.id.as_str()).copied().unwrap_or(0);
            let last = latest.get(c.id.as_str()).map(crate::inbox::preview_of);
            json!({"channel": c, "unread_count": n, "latest": last})
        })
        .collect();
    Ok(Json(json!({"dms": rows})))
}

/// Idempotent on the participant set: same members = same conversation.
async fn open_dm(
    State(state): State<Shared>,
    crate::Body(body): crate::Body<OpenDmBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if body.members.is_empty() {
        return Err(bad_request(bisa_core::text!(
            "error-node-collab-message-needs-least-one-recipient"
        )));
    }
    let members = parse_principals(&body.members)?;
    let channel = bisa_engine::messaging::open_dm(state.engine.inner(), &members)?;
    Ok(Json(json!({"channel": channel})))
}

// --- reactions / retractions ------------------------------------------------

async fn retract(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let event = bisa_engine::messaging::retract(state.engine.inner(), &id)?;
    Ok(Json(json!({"id": event})))
}

async fn react(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
    crate::Body(body): crate::Body<ReactBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if body.emoji.trim().is_empty() || body.emoji.chars().count() > 64 {
        return Err(bad_request(bisa_core::text!(
            "error-node-collab-emoji-must-be-1-64-chars"
        )));
    }
    // A person reacting from the app reacts as themselves.
    let event = bisa_engine::messaging::react(state.engine.inner(), &id, body.emoji.trim())?;
    Ok(Json(json!({"id": event})))
}

// --- read markers -----------------------------------------------------------

async fn mark_read(
    State(state): State<Shared>,
    crate::Body(body): crate::Body<ScopeBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    bisa_engine::messaging::mark_read(state.engine.inner(), &body.scope)?;
    Ok(Json(json!({"ok": true})))
}

async fn mark_unread(
    State(state): State<Shared>,
    crate::Body(body): crate::Body<ScopeBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    bisa_engine::messaging::mark_unread(state.engine.inner(), &body.scope)?;
    Ok(Json(json!({"ok": true})))
}

/// The routes this module mounts, for `bisa-node --bin api-docs` and the
/// route test. Kept beside `routes()` so a route added here is added here.
pub const ROUTES: &[RouteDoc] = &[
    RouteDoc {
        method: "GET",
        path: "/channels",
        summary: "Standing channels with unread counts and each room's latest live post (`latest`: `{author, snippet, at}`, `null` when nothing was said yet — a retracted post and a membership event are nobody's last words); `general` is always present.",
    },
    RouteDoc {
        method: "GET",
        path: "/messages/{id}",
        summary: "One message wherever it lives, with its attachments and artifacts and whether each is on this machine; 404 when unknown.",
    },
    RouteDoc {
        method: "POST",
        path: "/channels",
        summary: "Create a standing channel: `{name, topic?, agents?, teams?, humans?, tags?}` — `humans` are the pubkeys of hosted people on the roster, what a guest reaches.",
    },
    RouteDoc {
        method: "GET",
        path: "/channels/{id}",
        summary: "One channel with its roster's pubkeys and its derived members.",
    },
    RouteDoc {
        method: "PATCH",
        path: "/channels/{id}",
        summary: "Edit topic, roster (`agents`, `teams`, `humans` — each half replaced when present) and tags. Refused on a direct channel.",
    },
    RouteDoc {
        method: "DELETE",
        path: "/channels/{id}",
        summary: "Delete a channel, stopping any turn an agent runs in it; `general` answers 409, and so does a channel a workflow step still names. Its message log stays on disk as history.",
    },
    RouteDoc {
        method: "GET",
        path: "/channels/{id}/messages",
        summary: "Messages in the channel (`?before=&before_id=&limit=`), with reactions; 404 for a channel that is gone, whatever its log still holds.",
    },
    RouteDoc {
        method: "POST",
        path: "/channels/{id}/messages",
        summary: "Post a message: `{content, reply_to?, mentions?, attachments?, artifacts?}` — words or nothing (blank `content` with nothing attached is a 400), at most 256 KiB of them (a 400 in words, never a bare 413: what is a file goes as an attachment); an artifact is an uploaded file described with its title and kind, rendered live where it is read.",
    },
    RouteDoc {
        method: "GET",
        path: "/dms",
        summary: "Direct channels with unread counts and each one's latest live post (`latest`, as `GET /channels` carries it).",
    },
    RouteDoc {
        method: "POST",
        path: "/dms",
        summary: "Open a direct channel: `{members}` — the participants' pubkeys, the caller always included. Idempotent on the participant set.",
    },
    RouteDoc {
        method: "POST",
        path: "/messages/{id}/retract",
        summary: "Retract a message you authored.",
    },
    RouteDoc {
        method: "POST",
        path: "/messages/{id}/react",
        summary: "React to a message: `{emoji}`.",
    },
    RouteDoc {
        method: "POST",
        path: "/reactions/{id}/retract",
        summary: "Retract a reaction.",
    },
    RouteDoc {
        method: "POST",
        path: "/read",
        summary: "Mark a scope read: `{scope}`.",
    },
    RouteDoc {
        method: "POST",
        path: "/unread",
        summary: "Mark a scope unread: `{scope}`.",
    },
];
