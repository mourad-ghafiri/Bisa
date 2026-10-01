//! The folder repositories over HTTP — the notes' under `/notes/git…`, the
//! drawings' under `/drawings/git…` — one set of handler bodies over
//! [`bisa_engine::folder_git::FolderGit`], each route family mounting them
//! under its own prefix and documenting them in its own `ROUTES` table.

use crate::dto::{FolderRepo, RepoCommitBody, RepoCommitRow, RepoIdentityBody, RepoRemoteBody};
use crate::{ApiError, Shared};
use axum::Json;
use bisa_engine::folder_git::{Folder, FolderGit, RepoStatus};
use serde_json::json;

fn repo_of(state: &Shared, folder: Folder) -> &FolderGit {
    let inner = state.engine.inner();
    match folder {
        Folder::Notes => &inner.notes_git,
        Folder::Drawings => &inner.drawings_git,
    }
}

pub(crate) fn repo_row(s: RepoStatus) -> FolderRepo {
    FolderRepo {
        changed: s.changed,
        branch: s.branch,
        upstream: s.upstream,
        ahead: s.ahead,
        behind: s.behind,
        remote: s.remote,
        identity: s.identity.into(),
        last_commit: s.last_commit.map(|c| RepoCommitRow {
            short: c.short,
            subject: c.subject,
            at: c.at,
        }),
        in_progress: s.in_progress,
    }
}

pub(crate) async fn status(state: &Shared, folder: Folder) -> Result<Json<FolderRepo>, ApiError> {
    Ok(Json(repo_row(
        repo_of(state, folder).status(state.engine.inner()).await?,
    )))
}

pub(crate) async fn commit(
    state: &Shared,
    folder: Folder,
    body: RepoCommitBody,
) -> Result<Json<RepoCommitRow>, ApiError> {
    let c = repo_of(state, folder)
        .commit(state.engine.inner(), &body.message)
        .await?;
    Ok(Json(RepoCommitRow {
        short: c.short,
        subject: c.subject,
        at: c.at,
    }))
}

/// A commit message drafted by the General Agent from what changed —
/// read-only; nothing here commits. Always 200, in the shape the IDE's
/// suggestion answers: a refusal is a sentence in `error`, never a status.
pub(crate) async fn message(state: &Shared, folder: Folder) -> Json<serde_json::Value> {
    match repo_of(state, folder).suggest(state.engine.inner()).await {
        Ok(message) => Json(
            json!({"suggested": true, "message": message, "agent": bisa_core::AgentId::GENERAL, "error": serde_json::Value::Null}),
        ),
        Err(e) => Json(
            json!({"suggested": false, "message": serde_json::Value::Null, "agent": bisa_core::AgentId::GENERAL, "error": e.to_string()}),
        ),
    }
}

/// Push to `origin`, the person's own act — no Publish gate here.
pub(crate) async fn push(state: &Shared, folder: Folder) -> Result<Json<FolderRepo>, ApiError> {
    Ok(Json(repo_row(
        repo_of(state, folder).push(state.engine.inner()).await?,
    )))
}

pub(crate) async fn fetch(state: &Shared, folder: Folder) -> Result<Json<FolderRepo>, ApiError> {
    Ok(Json(repo_row(
        repo_of(state, folder).fetch(state.engine.inner()).await?,
    )))
}

/// Fast-forward to `origin`, consented: a safety ref first, like every
/// consented act. Answers `{recovery, pull, status}` ([`crate::dto::PullOutcomeBody`]).
pub(crate) async fn pull(
    state: &Shared,
    folder: Folder,
    headers: &axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let consent = crate::ide::consent::from_request(headers, state)?;
    let repo = repo_of(state, folder);
    let (recovery, outcome) = repo.pull(state.engine.inner(), consent).await?;
    let status = repo_row(repo.status(state.engine.inner()).await?);
    Ok(Json(
        json!({"recovery": recovery, "pull": outcome, "status": status}),
    ))
}

pub(crate) async fn set_remote(
    state: &Shared,
    folder: Folder,
    body: RepoRemoteBody,
) -> Result<Json<FolderRepo>, ApiError> {
    Ok(Json(repo_row(
        repo_of(state, folder)
            .set_remote(state.engine.inner(), &body.url)
            .await?,
    )))
}

pub(crate) async fn set_identity(
    state: &Shared,
    folder: Folder,
    body: RepoIdentityBody,
) -> Result<Json<FolderRepo>, ApiError> {
    Ok(Json(repo_row(
        repo_of(state, folder)
            .set_identity(state.engine.inner(), &body.name, &body.email)
            .await?,
    )))
}
