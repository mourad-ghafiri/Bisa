//! SSH for git hosts over HTTP (Settings › Git & code hosts › SSH keys, ide/04):
//! the public keys with whether ssh-agent holds them, the `Host` blocks of
//! `ssh_config` for git hosts, a new key pair, a key loaded into ssh-agent,
//! what `ssh -G` would offer a host, and one handshake a git host greets.
//! Public material only — a private key is never read by anything, and no
//! route returns one. An engine started without SSH answers 503 here.

use crate::dto::SshTestBody;
use crate::route_docs::RouteDoc;
use crate::Query;
use crate::{ApiError, Shared};
use axum::extract::{Path as AxPath, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use bisa_engine::ssh as eng;
use serde::Deserialize;
use std::path::PathBuf;

pub(crate) fn routes() -> Router<Shared> {
    Router::new()
        .route("/git/ssh", get(overview))
        .route("/git/ssh/keys", post(generate))
        .route("/git/ssh/keys/{name}/load", post(load))
        .route("/git/ssh/resolve", get(resolve))
        .route("/git/ssh/test", post(test))
}

async fn overview(State(state): State<Shared>) -> Result<Json<eng::SshOverview>, ApiError> {
    Ok(Json(eng::overview(state.engine.inner()).await?))
}

async fn generate(
    State(state): State<Shared>,
    crate::Body(body): crate::Body<eng::NewKey>,
) -> Result<(StatusCode, Json<eng::PublicKey>), ApiError> {
    let key = eng::generate(state.engine.inner(), body).await?;
    Ok((StatusCode::CREATED, Json(key)))
}

async fn load(
    State(state): State<Shared>,
    AxPath(name): AxPath<String>,
) -> Result<StatusCode, ApiError> {
    eng::load(state.engine.inner(), name).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct ResolveQuery {
    host: String,
    #[serde(default)]
    key: Option<String>,
}

async fn resolve(
    State(state): State<Shared>,
    Query(q): Query<ResolveQuery>,
) -> Result<Json<eng::Resolved>, ApiError> {
    Ok(Json(
        eng::resolve(state.engine.inner(), q.host, q.key.map(PathBuf::from)).await?,
    ))
}

async fn test(
    State(state): State<Shared>,
    crate::Body(body): crate::Body<SshTestBody>,
) -> Result<Json<eng::HostGreeting>, ApiError> {
    let user = body.user.unwrap_or_else(|| "git".to_string());
    Ok(Json(
        eng::test(
            state.engine.inner(),
            body.host,
            user,
            body.key.map(PathBuf::from),
        )
        .await?,
    ))
}

pub const ROUTES: &[RouteDoc] = &[
    RouteDoc { method: "GET", path: "/git/ssh", summary: "SSH for git hosts, in one read: the SSH directory (`dir`), every key pair with a `.pub` file (`keys[]`: `name`, `path`, `algorithm`, `comment`, `fingerprint`, the `public_line` to copy, and `loaded` — whether ssh-agent holds it), ssh-agent (`agent`: `available`, its `keys`), the `Host` blocks of `ssh_config` that concern git hosts (`hosts[]`), and the `known_git_hosts`. Public material only; 503 when this node was started without SSH." },
    RouteDoc { method: "POST", path: "/git/ssh/keys", summary: "Generate an ed25519 key pair in the SSH directory: `{name, comment?}` — `ssh-keygen -t ed25519`, no passphrase (one on argv would be visible to every process; add one with `ssh-keygen -p` in a terminal). A name already taken is refused before anything runs. 201 with the public key." },
    RouteDoc { method: "POST", path: "/git/ssh/keys/{name}/load", summary: "Load one key pair into ssh-agent (`ssh-add`, with the macOS keychain option on macOS). A key that wants a passphrase cannot be loaded without a prompt; the 400 names the command to run in a terminal. 204." },
    RouteDoc { method: "GET", path: "/git/ssh/resolve", summary: "What ssh would do for `?host=` — `ssh -G`, offline: the `hostname` an alias stands for, the `user`, the `port`, the `identity_files` it would offer, `identities_only`, the `identity_agent`; with `&key=` the profile’s key forced, as its `core.sshCommand` would." },
    RouteDoc { method: "POST", path: "/git/ssh/test", summary: "One authentication handshake with a git host — `ssh -T user@host` in batch mode, `{host, user?: git, key?}` — read into `{state: authenticated, login?}`, `{state: refused, reason}`, `{state: host_key_unknown, reason}` (nothing is accepted into `known_hosts` for you) or `{state: unreachable, reason}`. Nothing is written on either side." },
];
