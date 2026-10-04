//! The consented git routes (ide/04): everything that can move a workstream's
//! checkout — the project's own tree through its primary — plus the
//! safe reads the branch, tag and remote panels need.
//!
//! Every tree-moving handler mints a `HumanConsent` from the request first
//! (`consent::from_request`) and hands it to `bisa_engine::ide::interactive`,
//! which writes the recovery ref before it runs anything. The answer always
//! carries that recovery, so the client can offer *Restore what was here*.
//! A conflict or a refused-to-clobber is a 409 with git's own sentence.

use super::consent;
use crate::dto::{
    BranchCreateBody, BranchRenameBody, CheckoutBody, CherryPickBody, DiscardBody, ErrorCode,
    FetchBody, GitAmendBody, GitConflict, GitInProgress, GitMergePreview, GitOperationFacts,
    MergeBody, OperationBody, PullBody, RebaseBody, RebasePlanBody, RecoveryRefView, RemoteAddBody,
    RemoteSetBody, ResolveBody, RestoreBody, RevertBody, StashEntryView, StashPushBody,
    StashTargetBody, TagCreateBody, UpstreamBody,
};
use crate::projects::{gate_or_result, parse_workstream_id, Published};
use crate::route_docs::RouteDoc;
use crate::Query;
use crate::{bad_request, ApiError, Shared};
use axum::extract::{Path as AxPath, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use bisa_core::Localize as _;
use bisa_engine::ide::{git as safe, interactive as ide};
use bisa_engine::EngineError;
use bisa_vcs::VcsError;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

pub(crate) fn routes() -> Router<Shared> {
    Router::new()
        .route(
            "/workstreams/{wid}/git/branches",
            get(branches).post(branch_create),
        )
        .route(
            "/workstreams/{wid}/git/branches/{name}",
            axum::routing::delete(branch_delete),
        )
        .route(
            "/workstreams/{wid}/git/branches/{name}/rename",
            post(branch_rename),
        )
        .route(
            "/workstreams/{wid}/git/branches/{name}/upstream",
            axum::routing::put(branch_upstream),
        )
        .route(
            "/workstreams/{wid}/git/branches/{name}/commits",
            get(branch_commits),
        )
        .route(
            "/workstreams/{wid}/git/remote-branches",
            get(remote_branches),
        )
        .route(
            "/workstreams/{wid}/git/remote-branches/{remote}/{branch}",
            axum::routing::delete(remote_branch_delete),
        )
        .route("/workstreams/{wid}/git/checkout", post(checkout))
        .route("/workstreams/{wid}/git/tags", get(tags).post(tag_create))
        .route(
            "/workstreams/{wid}/git/tags/{name}",
            axum::routing::delete(tag_delete),
        )
        .route(
            "/workstreams/{wid}/git/remotes",
            get(remotes).post(remote_add),
        )
        .route(
            "/workstreams/{wid}/git/remotes/{name}",
            axum::routing::put(remote_set).delete(remote_remove),
        )
        .route("/workstreams/{wid}/git/fetch", post(fetch))
        .route("/workstreams/{wid}/git/pull", post(pull))
        .route("/workstreams/{wid}/git/amend", post(amend))
        .route("/workstreams/{wid}/git/rebase", post(rebase))
        .route("/workstreams/{wid}/git/rebase/plan", post(rebase_plan))
        .route("/workstreams/{wid}/git/merge", post(merge))
        .route("/workstreams/{wid}/git/cherry-pick", post(cherry_pick))
        .route("/workstreams/{wid}/git/revert", post(revert))
        .route("/workstreams/{wid}/git/abort", post(abort))
        .route("/workstreams/{wid}/git/continue", post(continue_op))
        .route("/workstreams/{wid}/git/skip", post(skip_op))
        .route("/workstreams/{wid}/git/conflict", get(conflict))
        .route("/workstreams/{wid}/git/operation", get(operation))
        .route("/workstreams/{wid}/git/merge-preview", get(merge_preview))
        .route("/workstreams/{wid}/git/resolve", post(resolve))
        .route("/workstreams/{wid}/git/discard", post(discard))
        .route("/workstreams/{wid}/git/recovery", get(recovery))
        .route("/workstreams/{wid}/git/recovery/restore", post(restore))
        .route("/workstreams/{wid}/git/stash", post(stash_push))
        .route("/workstreams/{wid}/git/stashes", get(stashes))
        .route("/workstreams/{wid}/git/stashes/{sha}/diff", get(stash_diff))
        .route(
            "/workstreams/{wid}/git/stashes/{sha}/apply",
            post(stash_apply),
        )
        .route("/workstreams/{wid}/git/stashes/{sha}/pop", post(stash_pop))
        .route(
            "/workstreams/{wid}/git/stashes/{sha}/drop",
            post(stash_drop),
        )
}

/// Git said no in a way the person has to act on: a conflict to resolve or
/// abort (named, with its paths and the operation left in progress), a pull
/// that cannot fast-forward (with the counts), an operation already half-done,
/// or local changes it will not overwrite. All are 409, not 400 — the request
/// was well formed; the tree is what stands in the way.
fn code(e: EngineError) -> ApiError {
    match &e {
        EngineError::Vcs(VcsError::Conflict {
            paths, in_progress, ..
        }) => ApiError::coded(StatusCode::CONFLICT, ErrorCode::Conflict, e.text()).with_detail(
            json!({
                "paths": paths,
                "in_progress": in_progress.map(GitInProgress::from),
            }),
        ),
        EngineError::Vcs(VcsError::NotFastForward { ahead, behind }) => {
            ApiError::coded(StatusCode::CONFLICT, ErrorCode::NotFastForward, e.text())
                .with_detail(json!({"ahead": ahead, "behind": behind}))
        }
        EngineError::Vcs(VcsError::InProgress(op)) => ApiError::coded(
            StatusCode::CONFLICT,
            ErrorCode::InProgress,
            bisa_core::text!("error-node-ide-interactive-refused", detail = e.to_string()),
        )
        .with_detail(json!({"in_progress": GitInProgress::from(*op)})),
        EngineError::Vcs(VcsError::Dirty { .. }) => ApiError::text(StatusCode::CONFLICT, e.text()),
        // Every selected path is gone, or never tracked: the tree's state, not the request.
        EngineError::Vcs(VcsError::NothingToDiscard) => {
            ApiError::text(StatusCode::CONFLICT, e.text())
        }
        EngineError::Vcs(VcsError::NothingToStash) => ApiError::coded(
            StatusCode::CONFLICT,
            ErrorCode::NothingToStash,
            bisa_core::text!("error-node-ide-interactive-refused", detail = e.to_string()),
        ),
        EngineError::Vcs(VcsError::StashMoved { index, commit, now }) => {
            ApiError::coded(StatusCode::CONFLICT, ErrorCode::StashMoved, e.text())
                .with_detail(json!({"index": index, "commit": commit, "now": now}))
        }
        _ => e.into(),
    }
}

fn done(wid: bisa_core::WorkstreamId, d: ide::Done) -> Json<serde_json::Value> {
    Json(json!({
        "workstream": wid.to_string(),
        "recovery": d.recovery,
        "branch": d.branch,
        "files": crate::projects::rows(d.files),
    }))
}

async fn branches(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let b = safe::branches(state.engine.inner(), wid).await?;
    Ok(Json(json!({"workstream": wid.to_string(), "branches": b})))
}

/// Remote-tracking branches as of the last fetch — a local read.
async fn remote_branches(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let b = safe::remote_branches(state.engine.inner(), wid).await?;
    Ok(Json(
        json!({"workstream": wid.to_string(), "remote_branches": b}),
    ))
}

/// Create a branch; with `switch`, check it out too (that half is consented).
async fn branch_create(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
    headers: HeaderMap,
    crate::Body(body): crate::Body<BranchCreateBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let consent = if body.switch {
        Some(consent::from_request(&headers, &state)?)
    } else {
        None
    };
    let branches = safe::branch_create(
        state.engine.inner(),
        wid,
        body.name.clone(),
        body.start,
        body.track,
    )
    .await?;
    match consent {
        Some(c) => {
            let d = ide::checkout(state.engine.inner(), wid, body.name, c)
                .await
                .map_err(code)?;
            Ok(done(wid, d))
        }
        None => Ok(Json(
            json!({"workstream": wid.to_string(), "branches": branches}),
        )),
    }
}

async fn branch_delete(
    State(state): State<Shared>,
    AxPath((wid, name)): AxPath<(String, String)>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let c = consent::from_request(&headers, &state)?;
    let d = ide::branch_delete(state.engine.inner(), wid, name, c)
        .await
        .map_err(code)?;
    Ok(done(wid, d))
}

async fn branch_rename(
    State(state): State<Shared>,
    AxPath((wid, name)): AxPath<(String, String)>,
    headers: HeaderMap,
    crate::Body(body): crate::Body<BranchRenameBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let c = consent::from_request(&headers, &state)?;
    let d = ide::rename_branch(state.engine.inner(), wid, name, body.to, c)
        .await
        .map_err(code)?;
    Ok(done(wid, d))
}

/// Point a branch at the upstream it follows, or at none. Safe: a line of config.
async fn branch_upstream(
    State(state): State<Shared>,
    AxPath((wid, name)): AxPath<(String, String)>,
    crate::Body(body): crate::Body<UpstreamBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    safe::set_upstream(state.engine.inner(), wid, name, body.upstream).await?;
    let b = safe::branches(state.engine.inner(), wid).await?;
    Ok(Json(json!({"workstream": wid.to_string(), "branches": b})))
}

#[derive(Deserialize)]
struct AgainstQuery {
    against: String,
    #[serde(default)]
    limit: Option<usize>,
}

/// How many commits a branch listing offers at most.
const BRANCH_COMMITS_CAP: usize = 200;

/// The commits `{name}` has that `?against=` lacks, newest first — what a
/// cherry-pick from the branch offers, what an interactive rebase replays.
async fn branch_commits(
    State(state): State<Shared>,
    AxPath((wid, name)): AxPath<(String, String)>,
    Query(q): Query<AgainstQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let limit = q
        .limit
        .unwrap_or(BRANCH_COMMITS_CAP)
        .min(BRANCH_COMMITS_CAP);
    let commits = safe::commits_between(state.engine.inner(), wid, name, q.against, limit).await?;
    Ok(Json(
        json!({"workstream": wid.to_string(), "commits": commits}),
    ))
}

/// Delete a branch on a remote — through the Publish gate like a push
/// (200 done, 202 gate open, 409 refused), consented, never the project's
/// default branch; the remote-tracking tip is pinned in Safety first.
async fn remote_branch_delete(
    State(state): State<Shared>,
    AxPath((wid, remote, branch)): AxPath<(String, String, String)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let consent = consent::from_request(&headers, &state)?;
    let inner = Arc::clone(state.engine.inner());
    match gate_or_result(&state, wid, async move {
        ide::push_delete(&inner, wid, remote, branch, consent).await
    })
    .await?
    {
        Published::Gate(gate) => Ok((
            StatusCode::ACCEPTED,
            Json(json!({
                "workstream": wid.to_string(),
                "gate": gate,
                "status": "awaiting_publish_gate",
                "deleted": false,
            })),
        )
            .into_response()),
        Published::Done(d) => Ok(done(wid, d).into_response()),
    }
}

async fn checkout(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
    headers: HeaderMap,
    crate::Body(body): crate::Body<CheckoutBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let c = consent::from_request(&headers, &state)?;
    let d = ide::checkout(state.engine.inner(), wid, body.target, c)
        .await
        .map_err(code)?;
    Ok(done(wid, d))
}

async fn tags(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let t = safe::tags(state.engine.inner(), wid).await?;
    Ok(Json(json!({"workstream": wid.to_string(), "tags": t})))
}

async fn tag_create(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
    headers: HeaderMap,
    crate::Body(body): crate::Body<TagCreateBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let c = consent::from_request(&headers, &state)?;
    let d = ide::tag_create(
        state.engine.inner(),
        wid,
        body.name,
        body.target,
        body.message,
        c,
    )
    .await
    .map_err(code)?;
    Ok(done(wid, d))
}

async fn tag_delete(
    State(state): State<Shared>,
    AxPath((wid, name)): AxPath<(String, String)>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let c = consent::from_request(&headers, &state)?;
    let d = ide::tag_delete(state.engine.inner(), wid, name, c)
        .await
        .map_err(code)?;
    Ok(done(wid, d))
}

async fn remotes(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let r = safe::remotes(state.engine.inner(), wid).await?;
    Ok(Json(json!({"workstream": wid.to_string(), "remotes": r})))
}

async fn remote_add(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
    crate::Body(body): crate::Body<RemoteAddBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    if body.url.trim().is_empty() {
        return Err(bad_request(bisa_core::text!(
            "error-node-ide-interactive-remote-needs-url"
        )));
    }
    let r = safe::remote_add(state.engine.inner(), wid, body.name, body.url).await?;
    Ok(Json(json!({"workstream": wid.to_string(), "remotes": r})))
}

/// Point a remote at a URL — adding it when it is not there. Safe:
/// configuration, not history. `origin` keeps the project record true.
async fn remote_set(
    State(state): State<Shared>,
    AxPath((wid, name)): AxPath<(String, String)>,
    crate::Body(body): crate::Body<RemoteSetBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    if body.url.trim().is_empty() {
        return Err(bad_request(bisa_core::text!(
            "error-node-ide-interactive-remote-needs-url"
        )));
    }
    let r = safe::set_remote(state.engine.inner(), wid, name, body.url.trim().to_string()).await?;
    Ok(Json(json!({"workstream": wid.to_string(), "remotes": r})))
}

/// Fetch a remote's refs. Safe — nothing in the tree moves — and the answer is
/// the checkout's fresh status, so ahead/behind reads true at once.
async fn fetch(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
    crate::Body(body): crate::Body<FetchBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let remote = body.remote.unwrap_or_else(|| "origin".to_string());
    safe::fetch(state.engine.inner(), wid, remote.clone()).await?;
    let status = safe::workstream_status(state.engine.inner(), wid).await?;
    Ok(Json(
        json!({"workstream": wid.to_string(), "remote": remote, "fetched": true, "status": status}),
    ))
}

/// Fetch, then move the branch to its upstream by `mode`. Consented — the
/// second half moves the tree — with a recovery ref first, like every verb here.
async fn pull(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
    headers: HeaderMap,
    crate::Body(body): crate::Body<PullBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let c = consent::from_request(&headers, &state)?;
    let remote = body.remote.unwrap_or_else(|| "origin".to_string());
    let pulled = ide::pull(state.engine.inner(), wid, remote, body.mode.into(), c)
        .await
        .map_err(code)?;
    Ok(Json(json!({
        "workstream": wid.to_string(),
        "recovery": pulled.done.recovery,
        "branch": pulled.done.branch,
        "files": crate::projects::rows(pulled.done.files),
        "pull": pulled.pull,
    })))
}

/// Rewrite the last commit with what is staged and a new message. Consented;
/// the old commit is pinned in Safety first, so the answer carries the
/// recovery beside the commit HEAD is now.
async fn amend(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
    headers: HeaderMap,
    crate::Body(body): crate::Body<GitAmendBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    if body.message.trim().is_empty() {
        return Err(bad_request(bisa_core::text!(
            "error-node-ide-interactive-commit-needs-message"
        )));
    }
    let c = consent::from_request(&headers, &state)?;
    let amended = ide::amend(
        state.engine.inner(),
        wid,
        body.message.trim().to_string(),
        body.paths,
        c,
    )
    .await
    .map_err(code)?;
    Ok(Json(json!({
        "workstream": wid.to_string(),
        "recovery": amended.done.recovery,
        "branch": amended.done.branch,
        "files": crate::projects::rows(amended.done.files),
        "commit": amended.commit.as_str(),
        "short": amended.commit.short(),
    })))
}

/// Undo one commit with a new one. Consented; a conflict is 409 like a
/// cherry-pick's, with the revert left in progress for `abort`.
async fn revert(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
    headers: HeaderMap,
    crate::Body(body): crate::Body<RevertBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let c = consent::from_request(&headers, &state)?;
    let d = ide::revert(state.engine.inner(), wid, body.into(), c)
        .await
        .map_err(code)?;
    Ok(done(wid, d))
}

async fn remote_remove(
    State(state): State<Shared>,
    AxPath((wid, name)): AxPath<(String, String)>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let c = consent::from_request(&headers, &state)?;
    let d = ide::remote_remove(state.engine.inner(), wid, name, c)
        .await
        .map_err(code)?;
    Ok(done(wid, d))
}

async fn rebase(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
    headers: HeaderMap,
    crate::Body(body): crate::Body<RebaseBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let c = consent::from_request(&headers, &state)?;
    let d = ide::rebase(state.engine.inner(), wid, body.into(), c)
        .await
        .map_err(code)?;
    Ok(done(wid, d))
}

/// An interactive rebase planned in full, with no terminal anywhere.
async fn rebase_plan(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
    headers: HeaderMap,
    crate::Body(body): crate::Body<RebasePlanBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let c = consent::from_request(&headers, &state)?;
    let d = ide::rebase_plan(state.engine.inner(), wid, body.into(), c)
        .await
        .map_err(code)?;
    Ok(done(wid, d))
}

async fn merge(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
    headers: HeaderMap,
    crate::Body(body): crate::Body<MergeBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let c = consent::from_request(&headers, &state)?;
    let d = ide::merge(
        state.engine.inner(),
        wid,
        body.source,
        body.mode.into(),
        body.message,
        c,
    )
    .await
    .map_err(code)?;
    Ok(done(wid, d))
}

async fn cherry_pick(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
    headers: HeaderMap,
    crate::Body(body): crate::Body<CherryPickBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let c = consent::from_request(&headers, &state)?;
    let d = ide::cherry_pick(state.engine.inner(), wid, body.into(), c)
        .await
        .map_err(code)?;
    Ok(done(wid, d))
}

async fn abort(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
    headers: HeaderMap,
    crate::Body(body): crate::Body<OperationBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let c = consent::from_request(&headers, &state)?;
    let d = ide::abort(state.engine.inner(), wid, body.what.into(), c)
        .await
        .map_err(code)?;
    Ok(done(wid, d))
}

/// Go on with the operation half-done once its conflicted paths are settled;
/// 409 `conflict` naming the paths that are not.
async fn continue_op(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
    headers: HeaderMap,
    crate::Body(body): crate::Body<OperationBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let c = consent::from_request(&headers, &state)?;
    let d = ide::continue_op(state.engine.inner(), wid, body.what.into(), c)
        .await
        .map_err(code)?;
    Ok(done(wid, d))
}

/// Leave out the commit a rebase, cherry-pick or revert stopped on.
async fn skip_op(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
    headers: HeaderMap,
    crate::Body(body): crate::Body<OperationBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let c = consent::from_request(&headers, &state)?;
    let d = ide::skip_op(state.engine.inner(), wid, body.what.into(), c)
        .await
        .map_err(code)?;
    Ok(done(wid, d))
}

#[derive(Deserialize)]
struct PathQuery {
    path: String,
}

/// A conflicted path whole (`?path=`): its kind, its three sides and the
/// file as git wrote it, for the block-by-block resolution.
async fn conflict(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
    Query(q): Query<PathQuery>,
) -> Result<Json<GitConflict>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let c = bisa_engine::ide::git::conflict(state.engine.inner(), wid, q.path.clone())
        .await
        .map_err(code)?;
    Ok(Json(GitConflict {
        workstream: wid.to_string(),
        path: q.path,
        kind: c.kind.map(Into::into),
        truncated: c.base.truncated || c.ours.truncated || c.theirs.truncated,
        base: c.base.text,
        ours: c.ours.text,
        theirs: c.theirs.text,
        text: c.text,
        hash: c.hash,
        binary: c.binary,
    }))
}

/// The operation half-done in the checkout, as facts; `null` when none is.
async fn operation(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
) -> Result<Json<Option<GitOperationFacts>>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let facts = bisa_engine::ide::git::operation(state.engine.inner(), wid)
        .await
        .map_err(code)?;
    Ok(Json(facts.map(Into::into)))
}

#[derive(serde::Deserialize)]
struct SourceQuery {
    source: String,
}

/// What merging `?source=` into HEAD would do, before anything moves.
async fn merge_preview(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
    Query(q): Query<SourceQuery>,
) -> Result<Json<GitMergePreview>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let preview = bisa_engine::ide::git::merge_preview(state.engine.inner(), wid, q.source.clone())
        .await
        .map_err(code)?;
    Ok(Json(match preview {
        Some(p) => GitMergePreview {
            workstream: wid.to_string(),
            source: q.source,
            supported: true,
            clean: p.clean,
            paths: p.paths.iter().map(|p| p.display().to_string()).collect(),
        },
        None => GitMergePreview {
            workstream: wid.to_string(),
            source: q.source,
            ..GitMergePreview::default()
        },
    }))
}

/// Settle a conflicted path: with `take`, a side taken whole and staged or
/// the path removed (consented — the tree moves); without, the merged text
/// the person saved is staged — *Mark resolved*, index only.
async fn resolve(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
    headers: HeaderMap,
    crate::Body(body): crate::Body<ResolveBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    match body.take {
        Some(how) => {
            let c = consent::from_request(&headers, &state)?;
            let d = ide::resolve(state.engine.inner(), wid, body.path, how.into(), c)
                .await
                .map_err(code)?;
            Ok(done(wid, d))
        }
        None => {
            let files =
                bisa_engine::projects::stage_in(state.engine.inner(), wid, vec![body.path]).await?;
            Ok(Json(
                json!({"workstream": wid.to_string(), "files": crate::projects::rows(files)}),
            ))
        }
    }
}

/// Throw away working-tree changes: one hunk (`patch`) or whole paths
/// (`paths`). Exactly one of the two.
async fn discard(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
    headers: HeaderMap,
    crate::Body(body): crate::Body<DiscardBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let c = consent::from_request(&headers, &state)?;
    let d = match (body.patch, body.paths) {
        (Some(patch), None) => ide::discard_hunk(state.engine.inner(), wid, patch, c).await,
        (None, Some(paths)) if !paths.is_empty() => {
            ide::discard_paths(state.engine.inner(), wid, paths, c).await
        }
        _ => {
            return Err(bad_request(bisa_core::text!(
                "error-node-ide-interactive-pass-exactly-one-patch-non-empty-paths"
            )))
        }
    }
    .map_err(code)?;
    Ok(done(wid, d))
}

async fn recovery(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let r: Vec<RecoveryRefView> = safe::recovery(state.engine.inner(), wid)
        .await?
        .into_iter()
        .map(Into::into)
        .collect();
    Ok(Json(json!({"workstream": wid.to_string(), "recovery": r})))
}

// ---------------------------------------------------------------------------
// Stash (ide/04 §Stash)
// ---------------------------------------------------------------------------

/// The stash list — the repository's, shared by every worktree of the project.
async fn stashes(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let s: Vec<StashEntryView> = safe::stashes(state.engine.inner(), wid)
        .await?
        .into_iter()
        .map(Into::into)
        .collect();
    Ok(Json(json!({"workstream": wid.to_string(), "stashes": s})))
}

/// One stash entry's patch, cut like a commit's.
async fn stash_diff(
    State(state): State<Shared>,
    AxPath((wid, sha)): AxPath<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let (diff, truncated) = safe::stash_diff(state.engine.inner(), wid, sha.clone()).await?;
    Ok(Json(json!({
        "workstream": wid.to_string(),
        "commit": sha,
        "diff": diff,
        "truncated": truncated,
    })))
}

/// Park the tree's changes as a stash entry. Consented; the tree is captured
/// first like every verb; 409 `nothing_to_stash` when nothing would be saved.
async fn stash_push(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
    headers: HeaderMap,
    crate::Body(body): crate::Body<StashPushBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let c = consent::from_request(&headers, &state)?;
    let stashed = ide::stash_push(state.engine.inner(), wid, body.into(), c)
        .await
        .map_err(code)?;
    Ok(Json(json!({
        "workstream": wid.to_string(),
        "recovery": stashed.done.recovery,
        "branch": stashed.done.branch,
        "files": crate::projects::rows(stashed.done.files),
        "stash": StashEntryView::from(stashed.stash),
    })))
}

fn stash_target(sha: String, body: StashTargetBody) -> ide::StashTarget {
    ide::StashTarget {
        index: body.index,
        commit: sha,
    }
}

async fn stash_apply(
    State(state): State<Shared>,
    AxPath((wid, sha)): AxPath<(String, String)>,
    headers: HeaderMap,
    crate::Body(body): crate::Body<StashTargetBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let c = consent::from_request(&headers, &state)?;
    let d = ide::stash_apply(state.engine.inner(), wid, stash_target(sha, body), c)
        .await
        .map_err(code)?;
    Ok(done(wid, d))
}

async fn stash_pop(
    State(state): State<Shared>,
    AxPath((wid, sha)): AxPath<(String, String)>,
    headers: HeaderMap,
    crate::Body(body): crate::Body<StashTargetBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let c = consent::from_request(&headers, &state)?;
    let d = ide::stash_pop(state.engine.inner(), wid, stash_target(sha, body), c)
        .await
        .map_err(code)?;
    Ok(done(wid, d))
}

async fn stash_drop(
    State(state): State<Shared>,
    AxPath((wid, sha)): AxPath<(String, String)>,
    headers: HeaderMap,
    crate::Body(body): crate::Body<StashTargetBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let c = consent::from_request(&headers, &state)?;
    let d = ide::stash_drop(state.engine.inner(), wid, stash_target(sha, body), c)
        .await
        .map_err(code)?;
    Ok(done(wid, d))
}

async fn restore(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
    headers: HeaderMap,
    crate::Body(body): crate::Body<RestoreBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let c = consent::from_request(&headers, &state)?;
    let d = ide::restore(state.engine.inner(), wid, body.r#ref, c)
        .await
        .map_err(code)?;
    Ok(done(wid, d))
}

pub const ROUTES: &[RouteDoc] = &[
    RouteDoc { method: "GET", path: "/workstreams/{wid}/git/branches", summary: "Local branches with tips, upstreams and the current one — and each one's standing: `ahead` / `behind` its upstream (0/0 for none or a gone one) and `merged` into the project's default branch." },
    RouteDoc { method: "GET", path: "/workstreams/{wid}/git/remote-branches", summary: "Remote-tracking branches — `{remote, name, head, subject, timestamp}` — newest first, as of the last fetch; `remote/HEAD` is not one. A local read: `POST …/git/fetch` brings the remote's news in." },
    RouteDoc { method: "POST", path: "/workstreams/{wid}/git/branches", summary: "Create a branch: `{name, start?, track?, switch?}`. Creating is safe; `track` makes `start` — a remote branch — its upstream (a config entry, still safe); `switch` checks it out and is consented." },
    RouteDoc { method: "PUT", path: "/workstreams/{wid}/git/branches/{name}/upstream", summary: "Set the branch's upstream to `{upstream}` — a remote branch — or none with `null`. Safe: a line of config, nothing moves." },
    RouteDoc { method: "GET", path: "/workstreams/{wid}/git/branches/{name}/commits", summary: "The commits `{name}` has that `?against=` lacks (`against..name`), newest first, at most 200 (`?limit=`) — what a cherry-pick from that branch or an interactive rebase onto `against` can take." },
    RouteDoc { method: "DELETE", path: "/workstreams/{wid}/git/remote-branches/{remote}/{branch}", summary: "Delete a branch on the remote: `git push <remote> --delete <branch>`. Consented, never the project's default branch, its tip pinned in Safety first, and **through the Publish gate** like a push: 200 when it left, 202 `awaiting_publish_gate` under `gated`, 409 `publish_manual` / `publish_no_goal`." },
    RouteDoc { method: "DELETE", path: "/workstreams/{wid}/git/branches/{name}", summary: "Delete a local branch. Consented; its tip is pinned in the recovery ref first." },
    RouteDoc { method: "POST", path: "/workstreams/{wid}/git/branches/{name}/rename", summary: "Rename a local branch to `{to}`. Consented; the old tip is pinned first; never the project's default branch; a workstream checked out on it follows." },
    RouteDoc { method: "POST", path: "/workstreams/{wid}/git/checkout", summary: "Move HEAD and the tree to `{target}` — a branch or a commit. Consented; a recovery ref is written first; 409 when local changes would be overwritten." },
    RouteDoc { method: "GET", path: "/workstreams/{wid}/git/tags", summary: "Tags, newest first, peeled to their commit." },
    RouteDoc { method: "POST", path: "/workstreams/{wid}/git/tags", summary: "Create a tag: `{name, target?, message?}` (annotated with a message). Consented." },
    RouteDoc { method: "DELETE", path: "/workstreams/{wid}/git/tags/{name}", summary: "Delete a tag. Consented; its target is pinned first." },
    RouteDoc { method: "GET", path: "/workstreams/{wid}/git/remotes", summary: "Remotes and their URLs." },
    RouteDoc { method: "POST", path: "/workstreams/{wid}/git/remotes", summary: "Add a remote: `{name, url}`." },
    RouteDoc { method: "PUT", path: "/workstreams/{wid}/git/remotes/{name}", summary: "Point a remote at `{url}`, adding it when it does not exist. Safe; `origin` also updates the project record." },
    RouteDoc { method: "DELETE", path: "/workstreams/{wid}/git/remotes/{name}", summary: "Remove a remote. Consented." },
    RouteDoc { method: "POST", path: "/workstreams/{wid}/git/fetch", summary: "Fetch `{remote?}` (`origin` by default). Safe; answers the checkout's fresh `status`." },
    RouteDoc { method: "POST", path: "/workstreams/{wid}/git/pull", summary: "Fetch, then move the branch to its upstream: `{mode: ff_only | rebase | merge, remote?}`. Consented; answers `pull: {upstream, from, to, moved}`. 409 `not_fast_forward` (`detail.ahead`/`behind`), 409 `conflict` (`detail.paths`, `detail.in_progress`), 409 `in_progress`." },
    RouteDoc { method: "POST", path: "/workstreams/{wid}/git/amend", summary: "Rewrite the last commit with what is staged (`paths` staged first, empty meaning what is already staged) and `{message}`: `git commit --amend`. Consented; the old commit is pinned in Safety first and answered as `recovery` beside `commit` and `short`. 409 `in_progress`; 400 on a detached HEAD, no commit yet, nobody set to commit, or a blank message." },
    RouteDoc { method: "POST", path: "/workstreams/{wid}/git/rebase", summary: "Rebase the current branch: `{upstream, onto?, autostash?}` — `git rebase [--autostash] [--onto <onto>] <upstream>`; with `onto`, only the commits since `upstream` are replayed there. Consented; 409 `conflict` with its paths, the rebase left in progress for `continue`, `skip` or `abort`." },
    RouteDoc { method: "POST", path: "/workstreams/{wid}/git/rebase/plan", summary: "An interactive rebase planned in full: `{upstream, onto?, steps: [{action: pick | reword | squash | fixup | drop, commit, message?}]}`, the steps oldest first and naming exactly the commits `upstream..HEAD` holds (at most 200), the first kept one never a squash or fixup, at least one kept; a reword or a squash carries its message. Consented; refused by name before anything is written; 409 `conflict` stops it as a rebase does." },
    RouteDoc { method: "POST", path: "/workstreams/{wid}/git/merge", summary: "Merge `{source}` into the current branch: `{source, mode?: ff | no_ff | ff_only | squash, message?}` — `squash` leaves the result staged and uncommitted; `message` names the merge commit. Consented; 409 `conflict` with its paths." },
    RouteDoc { method: "POST", path: "/workstreams/{wid}/git/cherry-pick", summary: "Cherry-pick `{commits}` (oldest first) onto the current branch, `{record_origin?}` (`-x`), `{no_commit?}`, `{mainline?}` for a merge commit. Consented; 409 `conflict` with its paths, the pick left in progress." },
    RouteDoc { method: "POST", path: "/workstreams/{wid}/git/revert", summary: "Undo `{commits}` with new commits, `{no_commit?}`, `{mainline?}` for a merge commit. Consented; 409 `conflict` with its paths." },
    RouteDoc { method: "POST", path: "/workstreams/{wid}/git/abort", summary: "Abandon an in-progress `{what: rebase | merge | cherry_pick | revert}`. Consented." },
    RouteDoc { method: "POST", path: "/workstreams/{wid}/git/continue", summary: "Finish the in-progress `{what: rebase | merge | cherry_pick | revert}` once every conflicted path is staged — `git <op> --continue` with no editor. Consented; 409 `conflict` naming the paths still unmerged, nothing written." },
    RouteDoc { method: "POST", path: "/workstreams/{wid}/git/skip", summary: "Skip the commit the in-progress `{what}` stopped on — rebase, cherry-pick or revert; a merge has nothing to skip (400). Consented." },
    RouteDoc { method: "GET", path: "/workstreams/{wid}/git/conflict", summary: "A conflicted path whole (`?path=`): its `kind` (`both_modified` · `both_added` · `both_deleted` · `deleted_by_us` · `deleted_by_them` · `added_by_us` · `added_by_them`), its three sides `base`, `ours`, `theirs` from the index's stages (git's words: under a rebase `ours` is the branch rebased onto; a side that is not there is null), the file as git wrote it with its markers (`text`) and its `hash` for the save that follows; `binary` when any is not text." },
    RouteDoc { method: "GET", path: "/workstreams/{wid}/git/operation", summary: "The operation git has left half-done, as facts read from its directory — `{kind, branch, ours: {role, name, commit, subject}, theirs: {…}, step: {done, total}}` — so one started in a terminal is described too; `null` when none is. `ours`/`theirs` are git's: under a rebase `ours` is the branch rebased onto and `theirs` the commit replayed." },
    RouteDoc { method: "GET", path: "/workstreams/{wid}/git/merge-preview", summary: "What merging `?source=` into HEAD would do, before anything moves: `{supported, clean, paths}` — the paths that would conflict, from `git merge-tree --write-tree` (objects only; no tree, no index); `supported: false` on a git before 2.38. For a rebase the answer over the two tips is a likelihood, not a promise." },
    RouteDoc { method: "POST", path: "/workstreams/{wid}/git/resolve", summary: "Settle a conflicted path: `{path}` alone stages the merged text already saved (index only, safe); `{path, take: ours | theirs}` takes that side whole — `git checkout --<side> -- <path>`, then staged — and `{path, take: delete}` removes the path (`git rm`), the answer to a side that deleted it; consented, for a binary, a deleted-by-one-side path, or a file too large to edit." },
    RouteDoc { method: "POST", path: "/workstreams/{wid}/git/discard", summary: "Throw away working-tree changes: `{patch}` for one hunk, or `{paths}`. Consented; what was there is in the recovery ref. A path the index no longer holds — gone since it was listed, or never tracked — is left out; 409 when none is left (nothing to discard), before any recovery ref is written." },
    RouteDoc { method: "GET", path: "/workstreams/{wid}/git/recovery", summary: "Recovery points under `refs/bisa/safety/`, newest first: op, branch, and `kind` — `commit` (a tip), `tree` (the saved index and working tree), `stash` (a dropped or popped stash entry)." },
    RouteDoc { method: "POST", path: "/workstreams/{wid}/git/recovery/restore", summary: "Put back what a recovery `{ref}` saved: a `commit` — HEAD to its branch or commit; a `tree` — the base checked out, the saved index and tree on top; a `stash` — the entry back on the stash list, the tree untouched. Consented and itself recorded." },
    RouteDoc { method: "POST", path: "/workstreams/{wid}/git/stash", summary: "Park the working tree's changes as a stash entry: `{message?, include_untracked?, keep_index?, paths?}` — `include_untracked` is `-u` (never ignored files), `paths` scopes it. Consented; the tree is captured first; answers the entry as `stash`. 409 `nothing_to_stash` when nothing would be saved (a clean tree, untracked files only, no commit yet), 409 `in_progress`, 409 on unmerged paths." },
    RouteDoc { method: "GET", path: "/workstreams/{wid}/git/stashes", summary: "The stash list, newest first: `index` (what `stash@{n}` means right now), `commit`, `branch`, `message` (the person's, or null for git's `WIP on …`), `subject`, `at`, `untracked`. The list is the repository's — every workstream of the project shares it." },
    RouteDoc { method: "GET", path: "/workstreams/{wid}/git/stashes/{sha}/diff", summary: "One stash entry's patch: the tracked change against the commit it was made on, plus the untracked files it carries as additions. Cut at 2 MiB (`truncated`)." },
    RouteDoc { method: "POST", path: "/workstreams/{wid}/git/stashes/{sha}/apply", summary: "Apply a stash entry onto the tree and keep it on the list: `{index}` with the `sha` in the path. Consented; 409 `stash_moved` (`detail.now`) when `stash@{index}` no longer holds the sha; 409 `conflict` with `detail.paths` and no `in_progress` — settle or discard the paths, there is nothing to abort; 409 on untracked files that already exist." },
    RouteDoc { method: "POST", path: "/workstreams/{wid}/git/stashes/{sha}/pop", summary: "Apply a stash entry and drop it — only when the apply went cleanly; a conflict keeps the entry. `{index}`. Consented; the entry is pinned as a `stash` recovery first. The same 409s as apply." },
    RouteDoc { method: "POST", path: "/workstreams/{wid}/git/stashes/{sha}/drop", summary: "Drop a stash entry: `{index}`. Consented; its commit is pinned as a `stash` recovery first, so *Restore* puts it back on the list. 409 `stash_moved`." },
];
