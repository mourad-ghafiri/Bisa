//! Collaboration (14-collaboration): the people this workspace hosts, the
//! invitations that bring them, the messages the classifier holds, the
//! workspaces this node is a guest of, and the wire itself.
//!
//! Everything about *this* workspace — invitations, people, held messages
//! — is the engine's and the store's, answered here directly. Everything
//! about the **wire** and the **hosted memberships** — relay health, a join,
//! a guest's channels and messages — belongs to the process that runs the
//! relay pool and the guest sessions, which lends it to the node as
//! [`CollabDoors`] the way an attachment fetch is lent. A node started
//! without one answers those routes with a 409 that says so, and lists no
//! hosts.

use crate::dto::*;
use crate::route_docs::RouteDoc;
use crate::Query;
use crate::{bad_request, channel_id, conflict, not_found, ApiError, Shared};
use axum::extract::{Path as AxPath, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use bisa_collab::{Directory, RelayCheck};
use bisa_core::{InviteId, MemberRole, Permission, PrincipalId};
use bisa_guest::{Hosted, HostedMessage};
use futures::future::BoxFuture;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

/// Why a door refused.
#[derive(Debug)]
pub enum CollabRefusal {
    /// This node is not hosted on that workspace.
    NotHosted,
    /// The host, or the session, said no — in words for the person.
    Refused(String),
    /// The wire could not do it.
    Failed(String),
}

impl From<CollabRefusal> for ApiError {
    fn from(r: CollabRefusal) -> Self {
        match r {
            CollabRefusal::NotHosted => not_found(bisa_core::text!(
                "error-node-collab-node-not-member-workspace"
            )),
            CollabRefusal::Refused(words) => bad_request(bisa_core::text!(
                "error-node-collab-refused",
                detail = words.to_string()
            )),
            CollabRefusal::Failed(words) => ApiError::text(
                StatusCode::BAD_GATEWAY,
                bisa_core::text!("error-node-collab-failed", detail = words.to_string()),
            ),
        }
    }
}

/// What the process that owns the relay pool and the guest sessions lends
/// the node. Every method is a future so the CLI's implementation may await
/// the pool; the node never holds a lock across one.
pub trait CollabDoors: Send + Sync {
    /// The wire as it stands.
    fn sync(&self) -> BoxFuture<'_, SyncReport>;
    /// Try one relay once, before it is added.
    fn check_relay(&self, url: String) -> BoxFuture<'_, RelayCheck>;
    /// Drop every relay connection and open them again.
    fn reconnect(&self) -> BoxFuture<'_, ()>;
    /// Every workspace this node is a guest of.
    fn hosts(&self) -> BoxFuture<'_, Vec<Hosted>>;
    /// Claim an invite code; the code's relays join the pool.
    fn join(
        &self,
        code: String,
        label: Option<String>,
    ) -> BoxFuture<'_, Result<Hosted, CollabRefusal>>;
    fn leave(&self, host: String) -> BoxFuture<'_, Result<(), CollabRefusal>>;
    fn hosted_channels(
        &self,
        host: String,
    ) -> BoxFuture<'_, Result<Vec<HostedChannelRow>, CollabRefusal>>;
    fn hosted_dms(
        &self,
        host: String,
    ) -> BoxFuture<'_, Result<Vec<HostedChannelRow>, CollabRefusal>>;
    fn hosted_members(&self, host: String) -> BoxFuture<'_, Result<Vec<Directory>, CollabRefusal>>;
    fn hosted_messages(
        &self,
        host: String,
        scope: String,
        before: Option<u64>,
        limit: usize,
    ) -> BoxFuture<'_, Result<Vec<HostedMessage>, CollabRefusal>>;
    fn hosted_post(
        &self,
        host: String,
        scope: String,
        post: HostedPostBody,
    ) -> BoxFuture<'_, Result<HostedPosted, CollabRefusal>>;
    fn hosted_react(
        &self,
        host: String,
        event: String,
        emoji: String,
    ) -> BoxFuture<'_, Result<HostedPosted, CollabRefusal>>;
    fn hosted_retract(
        &self,
        host: String,
        event: String,
    ) -> BoxFuture<'_, Result<HostedPosted, CollabRefusal>>;
    fn hosted_open_dm(
        &self,
        host: String,
        participants: Vec<String>,
    ) -> BoxFuture<'_, Result<(), CollabRefusal>>;
    fn hosted_read(&self, host: String, scope: String) -> BoxFuture<'_, Result<(), CollabRefusal>>;
}

pub type Collab = Arc<dyn CollabDoors>;

pub(crate) fn routes() -> Router<Shared> {
    Router::new()
        .route("/workspace/roles", get(roles))
        .route("/workspace/me", put(me))
        .route("/workspace/people", get(people).post(add_person))
        .route("/workspace/people/{pubkey}", delete(remove_person))
        .route("/workspace/people/{pubkey}/role", put(set_role))
        .route("/workspace/invites", get(invites).post(create_invite))
        .route("/workspace/invites/{id}", delete(revoke_invite))
        .route("/workspace/invites/{id}/admit", post(admit_invite))
        .route("/workspace/invites/{id}/refuse", post(refuse_invite))
        .route("/messages/held", get(held))
        .route("/messages/{id}/release", post(release))
        .route("/hosts", get(hosts))
        .route("/hosts/join", post(join))
        .route("/hosts/{host}", delete(leave))
        .route("/hosts/{host}/channels", get(hosted_channels))
        .route("/hosts/{host}/dms", get(hosted_dms).post(hosted_open_dm))
        .route("/hosts/{host}/members", get(hosted_members))
        .route(
            "/hosts/{host}/channels/{id}/messages",
            get(hosted_messages).post(hosted_post),
        )
        .route("/hosts/{host}/messages/{id}/react", post(hosted_react))
        .route("/hosts/{host}/messages/{id}/retract", post(hosted_retract))
        .route("/hosts/{host}/read", post(hosted_read))
        .route("/sync", get(sync))
        .route("/sync/relays/check", post(check_relay))
        .route("/sync/relays/reconnect", post(reconnect))
}

fn doors(state: &Shared) -> Result<&Collab, ApiError> {
    state.collab.as_ref().ok_or_else(|| {
        conflict(bisa_core::text!(
            "error-node-collab-node-runs-no-collaboration-pump-so-there"
        ))
    })
}

fn pubkey(hex: &str) -> Result<PrincipalId, ApiError> {
    PrincipalId::new(hex.trim().to_lowercase()).map_err(|e| {
        bad_request(bisa_core::text!(
            "error-node-collab-bad-pubkey",
            e = e.to_string()
        ))
    })
}

fn hosted_role(word: &str) -> Result<MemberRole, ApiError> {
    let role = word.parse::<MemberRole>().map_err(|e| {
        bad_request(bisa_core::text!(
            "error-node-collab-refused",
            detail = e.to_string()
        ))
    })?;
    if !role.is_hosted() {
        return Err(bad_request(bisa_core::text!(
            "error-node-collab-person-another-node-admin-member-guest-never"
        )));
    }
    Ok(role)
}

fn invite_id(s: &str) -> Result<InviteId, ApiError> {
    s.parse::<InviteId>().map_err(|e| {
        bad_request(bisa_core::text!(
            "error-node-collab-bad-invite-id",
            e = e.to_string()
        ))
    })
}

// -- roles and people -----------------------------------------------------------

/// The four roles and what each may do — the matrix, from the one place it
/// is written, so a screen renders it rather than restating it.
async fn roles() -> Json<serde_json::Value> {
    let roles: Vec<RoleRow> = MemberRole::ALL
        .iter()
        .map(|r| RoleRow {
            role: *r,
            words: r.words().to_string(),
            permissions: r.permissions().to_vec(),
        })
        .collect();
    let permissions: Vec<PermissionRow> = Permission::ALL
        .iter()
        .map(|p| PermissionRow {
            permission: *p,
            words: p.words().to_string(),
        })
        .collect();
    Json(json!({"roles": roles, "permissions": permissions}))
}

fn person_rows(state: &Shared) -> Result<Vec<PersonRow>, ApiError> {
    let ws = state.engine.workspace();
    let channels = ws.list_channels()?;
    Ok(ws
        .people()?
        .into_iter()
        .map(|m| {
            let rostered: Vec<String> = channels
                .iter()
                .filter(|c| c.roster.lists_human(&m.pubkey))
                .map(|c| c.id.to_string())
                .collect();
            PersonRow {
                permissions: m.role.permissions().to_vec(),
                channels: rostered,
                member: m,
            }
        })
        .collect())
}

async fn people(State(state): State<Shared>) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(json!({"people": person_rows(&state)?})))
}

/// The owner's own profile — the label and the face this node's lists draw
/// and every host is told (14-collaboration). The face is a picture this
/// machine holds within `MAX_FACE_BYTES`, refused by name otherwise.
async fn me(
    State(state): State<Shared>,
    crate::Body(body): crate::Body<MeBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if let Some(Some(photo)) = &body.photo {
        crate::attachments::photo_check(
            state.engine.workspace(),
            photo,
            bisa_core::PhotoProfile::Face,
        )?;
    }
    let me = bisa_engine::collab::set_profile(state.engine.inner(), body.label, body.photo)?;
    Ok(Json(json!({"me": me})))
}

async fn add_person(
    State(state): State<Shared>,
    crate::Body(body): crate::Body<PersonBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pk = pubkey(&body.pubkey)?;
    let role = hosted_role(body.role.as_deref().unwrap_or("guest"))?;
    let person = bisa_engine::collab::add_person(state.engine.inner(), pk, role, body.label)?;
    Ok(Json(
        json!({"person": person, "people": person_rows(&state)?}),
    ))
}

async fn set_role(
    State(state): State<Shared>,
    AxPath(hex): AxPath<String>,
    crate::Body(body): crate::Body<RoleBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pk = pubkey(&hex)?;
    let role = hosted_role(&body.role)?;
    let person = bisa_engine::collab::set_role(state.engine.inner(), &pk, role)?;
    Ok(Json(
        json!({"person": person, "people": person_rows(&state)?}),
    ))
}

async fn remove_person(
    State(state): State<Shared>,
    AxPath(hex): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pk = pubkey(&hex)?;
    bisa_engine::collab::remove_person(state.engine.inner(), &pk)?;
    Ok(Json(json!({"removed": pk, "people": person_rows(&state)?})))
}

// -- invitations ----------------------------------------------------------------

async fn invites(State(state): State<Shared>) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        json!({"invites": state.engine.workspace().invites()?}),
    ))
}

async fn create_invite(
    State(state): State<Shared>,
    crate::Body(body): crate::Body<NewInviteBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let role = hosted_role(body.role.as_deref().unwrap_or("guest"))?;
    let channels = body
        .channels
        .iter()
        .map(|c| channel_id(c))
        .collect::<Result<Vec<_>, _>>()?;
    let (invite, code) =
        bisa_engine::collab::create_invite(state.engine.inner(), role, channels, body.label)?;
    let link = code.link().map_err(|e| ApiError::internal(&e))?;
    let text = code.text().map_err(|e| ApiError::internal(&e))?;
    Ok(Json(json!({"invite": invite, "link": link, "code": text})))
}

async fn revoke_invite(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let invite = bisa_engine::collab::revoke_invite(state.engine.inner(), invite_id(&id)?)?;
    Ok(Json(json!({"invite": invite})))
}

async fn admit_invite(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let invite = bisa_engine::collab::settle_invite(state.engine.inner(), invite_id(&id)?, true)?;
    Ok(Json(json!({"invite": invite})))
}

async fn refuse_invite(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let invite = bisa_engine::collab::settle_invite(state.engine.inner(), invite_id(&id)?, false)?;
    Ok(Json(json!({"invite": invite})))
}

// -- held messages --------------------------------------------------------------

async fn held(State(state): State<Shared>) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        json!({"held": state.engine.workspace().held_messages()?}),
    ))
}

async fn release(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    bisa_engine::collab::release_message(state.engine.inner(), &id)?;
    Ok(Json(json!({"released": id})))
}

// -- hosted memberships ---------------------------------------------------------

async fn hosts(State(state): State<Shared>) -> Json<serde_json::Value> {
    let hosts = match state.collab.as_ref() {
        Some(doors) => doors.hosts().await,
        None => Vec::new(),
    };
    Json(json!({"hosts": hosts}))
}

async fn join(
    State(state): State<Shared>,
    crate::Body(body): crate::Body<JoinHostBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let code = body.code.trim().to_string();
    if code.is_empty() {
        return Err(bad_request(bisa_core::text!(
            "error-node-collab-paste-invite-link-code"
        )));
    }
    let hosted = doors(&state)?.join(code, body.label).await?;
    Ok(Json(json!({"host": hosted})))
}

async fn leave(
    State(state): State<Shared>,
    AxPath(host): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let host = pubkey(&host)?;
    doors(&state)?.leave(host.as_hex().to_string()).await?;
    Ok(Json(json!({"left": host})))
}

async fn hosted_channels(
    State(state): State<Shared>,
    AxPath(host): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let host = pubkey(&host)?;
    let channels = doors(&state)?
        .hosted_channels(host.as_hex().to_string())
        .await?;
    Ok(Json(json!({"channels": channels})))
}

async fn hosted_dms(
    State(state): State<Shared>,
    AxPath(host): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let host = pubkey(&host)?;
    let dms = doors(&state)?.hosted_dms(host.as_hex().to_string()).await?;
    Ok(Json(json!({"dms": dms})))
}

async fn hosted_members(
    State(state): State<Shared>,
    AxPath(host): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let host = pubkey(&host)?;
    let members = doors(&state)?
        .hosted_members(host.as_hex().to_string())
        .await?;
    Ok(Json(json!({"members": members})))
}

#[derive(Deserialize)]
struct PageQuery {
    #[serde(default)]
    before: Option<u64>,
    #[serde(default)]
    limit: Option<usize>,
}

async fn hosted_messages(
    State(state): State<Shared>,
    AxPath((host, id)): AxPath<(String, String)>,
    Query(q): Query<PageQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let host = pubkey(&host)?;
    let scope = channel_id(&id)?;
    let messages = doors(&state)?
        .hosted_messages(
            host.as_hex().to_string(),
            scope.to_string(),
            q.before,
            q.limit.unwrap_or(50).min(200),
        )
        .await?;
    Ok(Json(json!({"scope": scope, "messages": messages})))
}

async fn hosted_post(
    State(state): State<Shared>,
    AxPath((host, id)): AxPath<(String, String)>,
    crate::Body(body): crate::Body<HostedPostBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let host = pubkey(&host)?;
    let scope = channel_id(&id)?;
    if body.content.trim().is_empty() {
        return Err(bad_request(bisa_core::text!(
            "error-node-collab-empty-message"
        )));
    }
    for m in &body.mentions {
        pubkey(m)?;
    }
    let posted = doors(&state)?
        .hosted_post(host.as_hex().to_string(), scope.to_string(), body)
        .await?;
    Ok(Json(json!({"id": posted.id, "posted": posted})))
}

async fn hosted_react(
    State(state): State<Shared>,
    AxPath((host, id)): AxPath<(String, String)>,
    crate::Body(body): crate::Body<ReactBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let host = pubkey(&host)?;
    if body.emoji.trim().is_empty() || body.emoji.chars().count() > 64 {
        return Err(bad_request(bisa_core::text!(
            "error-node-collab-emoji-must-be-1-64-chars"
        )));
    }
    let posted = doors(&state)?
        .hosted_react(host.as_hex().to_string(), id, body.emoji.trim().to_string())
        .await?;
    Ok(Json(json!({"id": posted.id})))
}

async fn hosted_retract(
    State(state): State<Shared>,
    AxPath((host, id)): AxPath<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let host = pubkey(&host)?;
    let posted = doors(&state)?
        .hosted_retract(host.as_hex().to_string(), id)
        .await?;
    Ok(Json(json!({"id": posted.id})))
}

async fn hosted_open_dm(
    State(state): State<Shared>,
    AxPath(host): AxPath<String>,
    crate::Body(body): crate::Body<OpenDmBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let host = pubkey(&host)?;
    if body.members.is_empty() {
        return Err(bad_request(bisa_core::text!(
            "error-node-collab-message-needs-least-one-recipient"
        )));
    }
    let mut participants = Vec::new();
    for m in &body.members {
        participants.push(pubkey(m)?.as_hex().to_string());
    }
    doors(&state)?
        .hosted_open_dm(host.as_hex().to_string(), participants)
        .await?;
    Ok(Json(json!({"asked": true})))
}

async fn hosted_read(
    State(state): State<Shared>,
    AxPath(host): AxPath<String>,
    crate::Body(body): crate::Body<ScopeBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let host = pubkey(&host)?;
    let scope = channel_id(&body.scope)?;
    doors(&state)?
        .hosted_read(host.as_hex().to_string(), scope.to_string())
        .await?;
    Ok(Json(json!({"ok": true})))
}

// -- the wire -------------------------------------------------------------------

async fn sync(State(state): State<Shared>) -> Result<Json<SyncReport>, ApiError> {
    let report = match state.collab.as_ref() {
        Some(doors) => doors.sync().await,
        None => {
            let resolved = state.engine.workspace().settings(None)?;
            SyncReport::not_running(&bisa_core::SyncSettings::from_resolved(&resolved))
        }
    };
    Ok(Json(report))
}

async fn check_relay(
    State(state): State<Shared>,
    crate::Body(body): crate::Body<RelayUrlBody>,
) -> Result<Json<RelayCheck>, ApiError> {
    let url = body.url.trim().to_string();
    if !bisa_core::is_relay_url(&url) {
        return Err(bad_request(bisa_core::text!(
            "error-node-collab-relay-ws-wss-url"
        )));
    }
    Ok(Json(doors(&state)?.check_relay(url).await))
}

async fn reconnect(State(state): State<Shared>) -> Result<Json<serde_json::Value>, ApiError> {
    doors(&state)?.reconnect().await;
    Ok(Json(json!({"ok": true})))
}

/// The routes this module mounts, for `bisa-node --bin api-docs` and the
/// route test.
pub const ROUTES: &[RouteDoc] = &[
    RouteDoc {
        method: "GET",
        path: "/workspace/roles",
        summary: "The four roles — owner, admin, member, guest — each with its words and the permissions it holds, and every permission with its words: the matrix a screen renders.",
    },
    RouteDoc {
        method: "GET",
        path: "/workspace/people",
        summary: "The people hosted here (every member but the owner): each with its role, label, face (`photo`, set on their own node), who invited them, what client they said they use, the permissions of the role, and the standing channels a guest is rostered on.",
    },
    RouteDoc {
        method: "PUT",
        path: "/workspace/me",
        summary: "The owner's own profile: `{label?, photo?}` — each kept when absent, cleared with `null`; the face a picture this machine holds within `MAX_FACE_BYTES` (16 KiB), refused by name otherwise. What this node's lists draw for you and what every host you are a guest of is told (14-collaboration). Answers your row.",
    },
    RouteDoc {
        method: "POST",
        path: "/workspace/people",
        summary: "Admit a person by pubkey without an invite: `{pubkey, role?, label?}` — `role` is admin | member | guest (default). Answers the person and the people.",
    },
    RouteDoc {
        method: "PUT",
        path: "/workspace/people/{pubkey}/role",
        summary: "Change a person's role: `{role}` — admin | member | guest. Their reach follows; the host tells them.",
    },
    RouteDoc {
        method: "DELETE",
        path: "/workspace/people/{pubkey}",
        summary: "Remove a person: off every roster, their held messages dropped, their row gone; the host tells them. What they already received cannot be un-sent.",
    },
    RouteDoc {
        method: "GET",
        path: "/workspace/invites",
        summary: "Every invitation, newest first, with its state — pending, requested (waiting on you under `collab.join = ask`), accepted, refused, revoked, expired. Never a secret.",
    },
    RouteDoc {
        method: "POST",
        path: "/workspace/invites",
        summary: "Make a single-use invitation: `{role?, channels?, label?}` — the role granted (guest by default), the standing channels a guest is put on, what to call them. Answers the record, the `link` (`bisa://join/…`) and the `code` (`<nprofile>:<secret>`): the secret exists in this answer and nowhere else. Expires after `collab.invite_ttl_hours`.",
    },
    RouteDoc {
        method: "DELETE",
        path: "/workspace/invites/{id}",
        summary: "Withdraw a pending invitation; a code already claimed stays claimed.",
    },
    RouteDoc {
        method: "POST",
        path: "/workspace/invites/{id}/admit",
        summary: "Admit the person waiting on a requested invitation (`collab.join = ask`): a member at the invitation's role and channels; the host welcomes them.",
    },
    RouteDoc {
        method: "POST",
        path: "/workspace/invites/{id}/refuse",
        summary: "Turn down the person waiting on a requested invitation; the host tells them.",
    },
    RouteDoc {
        method: "GET",
        path: "/messages/held",
        summary: "Messages from people on other nodes that no agent may hear yet — waiting for the classifier, or held by it with the reason.",
    },
    RouteDoc {
        method: "POST",
        path: "/messages/{id}/release",
        summary: "Let a held message through: the agents it addresses are woken as if it had just arrived.",
    },
    RouteDoc {
        method: "GET",
        path: "/hosts",
        summary: "The workspaces this node is a guest of: each host's card, the role granted, where the membership stands (requested, member, refused, removed, left). Empty when the node runs no collaboration pump.",
    },
    RouteDoc {
        method: "POST",
        path: "/hosts/join",
        summary: "Join a workspace from an invitation: `{code, label?}` — the link or the text code. The code's relays join `sync.relays`; the claim is sent; answers the membership as it stands after the host's first word or the wait (`state`: member, requested, refused). 409 without a collaboration pump.",
    },
    RouteDoc {
        method: "DELETE",
        path: "/hosts/{host}",
        summary: "Leave a workspace: the host is told, the replica stays marked as left.",
    },
    RouteDoc {
        method: "GET",
        path: "/hosts/{host}/channels",
        summary: "The standing channels this person reaches on that host, each with its unread count.",
    },
    RouteDoc {
        method: "GET",
        path: "/hosts/{host}/dms",
        summary: "The direct channels this person is in on that host, each with its unread count.",
    },
    RouteDoc {
        method: "POST",
        path: "/hosts/{host}/dms",
        summary: "Ask the host for a direct channel with these people: `{members}` (pubkeys the host's directory names). The host answers with the channels; the new one appears among them.",
    },
    RouteDoc {
        method: "GET",
        path: "/hosts/{host}/members",
        summary: "The host's directory: every person there with their role and label.",
    },
    RouteDoc {
        method: "GET",
        path: "/hosts/{host}/channels/{id}/messages",
        summary: "Messages in a hosted channel (`?before=&limit=`), each with its reactions and whether it was retracted; attachments as descriptors, their bytes staying on the host.",
    },
    RouteDoc {
        method: "POST",
        path: "/hosts/{host}/channels/{id}/messages",
        summary: "Post into a hosted channel as this person: `{content, mentions?, reply_to?}` — text, pubkeys and a parent only; the text passes the redactor before it is signed. Refused for a channel the role does not reach.",
    },
    RouteDoc {
        method: "POST",
        path: "/hosts/{host}/messages/{id}/react",
        summary: "React to a hosted message: `{emoji}`.",
    },
    RouteDoc {
        method: "POST",
        path: "/hosts/{host}/messages/{id}/retract",
        summary: "Take back one of this person's own hosted messages or reactions.",
    },
    RouteDoc {
        method: "POST",
        path: "/hosts/{host}/read",
        summary: "Mark a hosted scope read: `{scope}`.",
    },
    RouteDoc {
        method: "GET",
        path: "/sync",
        summary: "The wire as it stands: whether a pump runs, whether `sync.enabled` is on, every configured relay in order with its health (status — `off` while the switch is off — attempts, success rate, latency, bytes), counts published and ingested, the last catch-up, people hosted here, hosts joined, and the direct transport's endpoint and sessions.",
    },
    RouteDoc {
        method: "POST",
        path: "/sync/relays/check",
        summary: "Try one relay once, on a throwaway connection, before it is added: `{url}` → `{ok, latency_ms?, error?}`. 400 for a URL that is not `ws(s)://`.",
    },
    RouteDoc {
        method: "POST",
        path: "/sync/relays/reconnect",
        summary: "Drop every relay connection and open them again.",
    },
];
