//! Git config, machine-wide: the person's **global** layer behind
//! Settings › Git & code hosts › Identity — read, and written there and only
//! there, at their request (I45) — the **profiles by organization** that the
//! global file includes by remote (ide/04 §Profiles by organization), and
//! what the ask dialog seeds from.
//!
//! A project's **local** layer is `projects.rs`'s (`/workstreams/{wid}/git/config`).

use crate::dto::{CommitterView, GitConfigView, GitConfigWrite};
use crate::i18n::Lang;
use crate::route_docs::RouteDoc;
use crate::{bad_request, ApiError, Shared};
use axum::extract::{Path as AxPath, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use bisa_core::Localize as _;
use bisa_core::{ProfileSpec, Slug};
use bisa_engine::gitprofiles;

pub(crate) fn routes() -> Router<Shared> {
    Router::new()
        .route("/git/config", get(global_config).put(set_global_config))
        .route("/git/committer", get(committer))
        .route("/git/profiles", get(profiles))
        .route(
            "/git/profiles/{slug}",
            axum::routing::put(put_profile).delete(delete_profile),
        )
}

/// `GET /git/config`: the schema and the global layer.
async fn global_config(
    State(state): State<Shared>,
    Lang(locale): Lang,
) -> Result<Json<GitConfigView>, ApiError> {
    let view = bisa_engine::identity::global_config(state.engine.inner()).await?;
    Ok(Json(GitConfigView::localized(view, &locale)))
}

/// `PUT /git/config`: the schema keys' global write.
async fn set_global_config(
    State(state): State<Shared>,
    Lang(locale): Lang,
    crate::Body(body): crate::Body<GitConfigWrite>,
) -> Result<Json<GitConfigView>, ApiError> {
    let (set, unset) = body.into_parts();
    let view = bisa_engine::identity::set_global_config(state.engine.inner(), set, unset).await?;
    Ok(Json(GitConfigView::localized(view, &locale)))
}

/// `GET /git/committer`: the global pair and the projects still asking.
async fn committer(State(state): State<Shared>) -> Result<Json<CommitterView>, ApiError> {
    let view = bisa_engine::identity::overview(state.engine.inner()).await?;
    Ok(Json(view.into()))
}

/// `GET /git/profiles`: every profile, the includes that are not ours, and
/// whether this git can evaluate them.
async fn profiles(
    State(state): State<Shared>,
) -> Result<Json<gitprofiles::GitProfilesView>, ApiError> {
    Ok(Json(gitprofiles::list(state.engine.inner()).await?))
}

fn parse_slug(slug: &str) -> Result<Slug, ApiError> {
    Slug::new(slug).map_err(|e| bad_request(e.text()))
}

/// `PUT /git/profiles/{slug}`: save a profile — its file and its includes.
async fn put_profile(
    State(state): State<Shared>,
    AxPath(slug): AxPath<String>,
    crate::Body(body): crate::Body<ProfileSpec>,
) -> Result<Json<gitprofiles::GitProfileView>, ApiError> {
    let slug = parse_slug(&slug)?;
    Ok(Json(
        gitprofiles::put(state.engine.inner(), slug, body).await?,
    ))
}

/// `DELETE /git/profiles/{slug}`: its includes, then its file. 204.
async fn delete_profile(
    State(state): State<Shared>,
    AxPath(slug): AxPath<String>,
) -> Result<StatusCode, ApiError> {
    let slug = parse_slug(&slug)?;
    gitprofiles::remove(state.engine.inner(), slug).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub const ROUTES: &[RouteDoc] = &[
    RouteDoc {
        method: "GET",
        path: "/git/config",
        summary: "The person’s global git config: the platform’s schema (`user.name`, `user.email`, `user.useConfigOnly`, `user.signingkey`, `commit.gpgsign`, `pull.rebase`, `core.autocrlf`, `init.defaultBranch`, `codehost.account`) and each key’s global value.",
    },
    RouteDoc {
        method: "PUT",
        path: "/git/config",
        summary: "Write the person’s global git config — the schema keys’ one write, at their request from Settings › Git & code hosts › Identity: `{set: {key: value}, unset: [key]}`, schema keys only; 400 names the first refusal. The platform’s profile includes are re-appended after it. Answers the fresh view.",
    },
    RouteDoc {
        method: "GET",
        path: "/git/committer",
        summary: "What the *who commits?* dialog seeds from: the global git pair and the projects still asking (`pending`: project, slug, primary workstream, reason). The answer is written per repository through `PUT /workstreams/{wid}/git/config`.",
    },
    RouteDoc {
        method: "GET",
        path: "/git/profiles",
        summary: "The git profiles by organization (ide/04): each with its `slug`, `label`, `host`, `owner`, `aliases`, `name`, `email`, `ssh_key`, `account`, its `file` under the workspace’s `identity/git/profiles/` and the `globs` its `includeIf \"hasconfig:remote.*.url:…\"` entries carry; the global file’s `foreign_includes` (a person’s own, left alone); `git_version` and whether `hasconfig_supported` (git ≥ 2.36).",
    },
    RouteDoc {
        method: "PUT",
        path: "/git/profiles/{slug}",
        summary: "Save a profile: `{label, host, owner, aliases?, name, email, ssh_key?, account?}`. The file is written owner-only and every glob’s include is set — last in the global file, so the profile outranks the global keys — and the stale ones for that file removed; a git older than 2.36 refuses before anything is written. Answers the profile.",
    },
    RouteDoc {
        method: "DELETE",
        path: "/git/profiles/{slug}",
        summary: "Remove a profile: its includes, then its file. 204; a slug with no profile is not an error.",
    },
];
