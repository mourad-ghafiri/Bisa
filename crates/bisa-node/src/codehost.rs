//! Pull requests over HTTP (ide/08): the linked pull request, its checks, a
//! review, the merge — through the Publish gate like a push — plus what the
//! code host behind a project can do, the **accounts** of each kind (their
//! logins and sources, never a token), each kind's health and its way to
//! sign in, and what a remote is before a project exists.

use crate::dto::{
    AddAccountBody, DefaultAccountBody, InspectBody, PrMergeBody, PrReviewBody, PrThreadReplyBody,
    PrThreadResolveBody,
};
use crate::projects::{
    gate_or_result, parse_project_id, parse_workstream_id, require_git_workstream, Published,
};
use crate::route_docs::RouteDoc;
use crate::{bad_request, ApiError, Shared};
use axum::extract::{Path as AxPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use bisa_engine::codehost as eng;
use serde_json::json;
use std::sync::Arc;

pub(crate) fn routes() -> Router<Shared> {
    Router::new()
        .route("/workstreams/{wid}/pr", get(linked_pr))
        .route("/workstreams/{wid}/pr/checks", get(checks))
        .route("/workstreams/{wid}/pr/reviews", get(pr_reviews))
        .route("/workstreams/{wid}/pr/review", post(review))
        .route(
            "/workstreams/{wid}/pr/threads/{thread_id}/resolve",
            post(resolve_thread),
        )
        .route(
            "/workstreams/{wid}/pr/threads/{thread_id}/reply",
            post(reply_thread),
        )
        .route("/workstreams/{wid}/pr/merge", post(merge))
        .route("/codehost/capabilities/{pid}", get(capabilities))
        .route("/projects/{pid}/prs", get(open_prs))
        .route("/codehost/inspect", post(inspect))
        .route("/codehost/{kind}/accounts", get(accounts).put(add_account))
        .route("/codehost/{kind}/accounts/{login}", delete(forget_account))
        .route(
            "/codehost/{kind}/accounts/{login}/check",
            post(check_account),
        )
        .route("/codehost/{kind}/default", put(set_default_account))
        .route("/codehost/{kind}/health", get(health))
        .route("/codehost/{kind}/login", get(login_plan))
}

/// The kind a path names, or a 404 that lists the three.
fn parse_kind(kind: &str) -> Result<eng::CodeHostKind, ApiError> {
    kind.parse::<eng::CodeHostKind>().map_err(|e| {
        ApiError::text(
            StatusCode::NOT_FOUND,
            bisa_core::text!("error-node-codehost-unknown-kind", detail = e.to_string()),
        )
    })
}

async fn linked_pr(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let pr = eng::linked_pr(state.engine.inner(), wid).await?;
    Ok(Json(json!({"workstream": wid.to_string(), "pr": pr})))
}

async fn checks(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let runs = eng::checks(state.engine.inner(), wid).await?;
    Ok(Json(json!({"workstream": wid.to_string(), "checks": runs})))
}

/// The PR's submitted reviews and resolvable threads, read from the code host.
async fn pr_reviews(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let reviews = eng::pr_reviews(state.engine.inner(), wid).await?;
    Ok(Json(json!({
        "workstream": wid.to_string(),
        "reviews": reviews.reviews,
        "threads": reviews.threads,
    })))
}

/// Resolve or unresolve one review thread on the PR. Not an outward
/// publish, so no gate.
async fn resolve_thread(
    State(state): State<Shared>,
    AxPath((wid, thread_id)): AxPath<(String, String)>,
    crate::Body(body): crate::Body<PrThreadResolveBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    eng::resolve_review_thread(state.engine.inner(), wid, &thread_id, body.resolved).await?;
    Ok(Json(json!({
        "workstream": wid.to_string(),
        "thread": thread_id,
        "resolved": body.resolved,
    })))
}

/// The person's reply on one review thread — and, with `resolve`, the
/// thread resolved in the same act. The route is the person's: an agent
/// replies through the `pr_thread_reply` MCP tool, signed. Empty words are a
/// 400 before the engine is asked.
async fn reply_thread(
    State(state): State<Shared>,
    AxPath((wid, thread_id)): AxPath<(String, String)>,
    crate::Body(body): crate::Body<PrThreadReplyBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    if body.body.trim().is_empty() {
        return Err(bad_request(bisa_core::text!(
            "error-node-codehost-reply-needs-words-say-what-you-changed"
        )));
    }
    eng::reply_review_thread(
        state.engine.inner(),
        wid,
        &thread_id,
        &body.body,
        body.resolve,
        eng::Reviewer::Person,
    )
    .await?;
    Ok(Json(json!({
        "workstream": wid.to_string(),
        "thread": thread_id,
        "replied": true,
        "resolved": body.resolve,
    })))
}

async fn review(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
    crate::Body(body): crate::Body<PrReviewBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    eng::review(
        state.engine.inner(),
        wid,
        eng::Review {
            event: body.event,
            body: body.body,
            comments: body.comments,
        },
        // The route is the person's: an agent reviews through the MCP tool.
        eng::Reviewer::Person,
    )
    .await?;
    Ok(Json(
        json!({"workstream": wid.to_string(), "reviewed": true}),
    ))
}

/// Merge — through the Publish gate: 200 merged, 202 gate open, 409 refused.
async fn merge(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
    crate::Body(body): crate::Body<PrMergeBody>,
) -> Result<Response, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let w = state.engine.workspace().get_workstream(wid)?;
    require_git_workstream(&w)?;
    let inner = Arc::clone(state.engine.inner());
    let strategy = body.strategy;
    let delete_branch = body.delete_branch;
    match gate_or_result(&state, wid, async move {
        eng::merge(&inner, wid, strategy, delete_branch).await
    })
    .await?
    {
        Published::Gate(gate) => Ok((
            StatusCode::ACCEPTED,
            Json(json!({
                "workstream": wid.to_string(),
                "gate": gate,
                "status": "awaiting_publish_gate",
                "merged": false,
            })),
        )
            .into_response()),
        Published::Done(out) => Ok(Json(json!({
            "workstream": wid.to_string(),
            "merged": out.merged,
            "sha": out.sha,
            "message": out.message,
            "remote_branch_deleted": out.remote_branch_deleted,
        }))
        .into_response()),
    }
}

/// What the code host behind a project's `origin` can do — the pull-request form
/// is rendered from this and nothing else. `code_host: null` for a project with
/// no remote or one on a code host this build does not know.
async fn capabilities(
    State(state): State<Shared>,
    AxPath(pid): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pid = parse_project_id(&pid)?;
    match eng::capabilities(state.engine.inner(), pid).await? {
        Some((id, repo, caps)) => Ok(Json(json!({
            "project": pid.to_string(),
            "code_host": id,
            "repo": repo,
            "capabilities": caps,
        }))),
        None => Ok(Json(json!({
            "project": pid.to_string(),
            "code_host": null,
            "repo": null,
            "capabilities": null,
        }))),
    }
}

/// The open pull requests on the code host behind the project's `origin` —
/// what the New workstream dialog offers to open a workstream from.
async fn open_prs(
    State(state): State<Shared>,
    AxPath(pid): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pid = parse_project_id(&pid)?;
    let prs = eng::list_open_prs(state.engine.inner(), pid).await?;
    Ok(Json(json!({ "project": pid.to_string(), "prs": prs })))
}

/// What a remote is before a project exists: `{url}` for a clone, `{path}`
/// for a folder the person picked — exactly one. Offline.
async fn inspect(
    State(state): State<Shared>,
    crate::Body(body): crate::Body<InspectBody>,
) -> Result<Json<eng::RemoteInspection>, ApiError> {
    let source = match (body.url, body.path) {
        (Some(url), None) if !url.trim().is_empty() => eng::InspectSource::Url(url),
        (None, Some(path)) if !path.trim().is_empty() => {
            // The same questions adopting the folder would ask — absolute,
            // real, a directory, outside the workspace — so the dialog that
            // leads to `adopt` cannot inspect what adopt then refuses.
            let root = state.engine.workspace().root().to_path_buf();
            let real = crate::projects::resolve_source_path(&root, &path, "inspect")
                .map_err(bad_request)?;
            eng::InspectSource::Path(std::path::PathBuf::from(real))
        }
        _ => {
            return Err(bad_request(bisa_core::text!(
                "error-node-codehost-one-url-path"
            )))
        }
    };
    Ok(Json(eng::inspect(state.engine.inner(), source).await?))
}

/// One kind's accounts: the stored logins and where each token lives, whether
/// the environment overrides them all, git's helpers and the username they
/// hold, the login the CLI is signed in as, and the default account. **Never
/// a token**, and no request to the host — git and the CLI are asked locally.
async fn accounts(
    State(state): State<Shared>,
    AxPath(kind): AxPath<String>,
) -> Result<Json<eng::AccountsView>, ApiError> {
    let kind = parse_kind(&kind)?;
    Ok(Json(eng::accounts(state.engine.inner(), kind).await?))
}

/// One kind's health for Settings: the CLI (installed, version, accounts),
/// the stored accounts, git's helper, the default, who would answer.
async fn health(
    State(state): State<Shared>,
    AxPath(kind): AxPath<String>,
) -> Result<Json<eng::CodeHostHealth>, ApiError> {
    let kind = parse_kind(&kind)?;
    Ok(Json(eng::health(state.engine.inner(), kind).await?))
}

/// How to sign in to one kind: the CLI's browser login (run in a terminal by
/// the desktop shell), the way to install the CLI, or a token from the host.
async fn login_plan(
    State(state): State<Shared>,
    AxPath(kind): AxPath<String>,
) -> Result<Json<eng::LoginPlan>, ApiError> {
    let kind = parse_kind(&kind)?;
    Ok(Json(eng::login_plan(state.engine.inner(), kind).await))
}

/// Ask the host whose `login`'s credential is: `connected` (login, scopes, the
/// required and recommended scopes it lacks, the organizations it sees),
/// `refused`, `unreachable`, or `no_token` without a request.
async fn check_account(
    State(state): State<Shared>,
    AxPath((kind, login)): AxPath<(String, String)>,
) -> Result<Json<eng::Connection>, ApiError> {
    let kind = parse_kind(&kind)?;
    Ok(Json(
        eng::check_account(state.engine.inner(), kind, &login).await?,
    ))
}

/// Add an account: the token is verified with the host first and kept under
/// the login the host answers. A refused token is a 401 and nothing is kept;
/// an accepted one answers `{login, connection}`. Bitbucket's Basic credential
/// needs the `login` beside the token.
async fn add_account(
    State(state): State<Shared>,
    AxPath(kind): AxPath<String>,
    crate::Body(body): crate::Body<AddAccountBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let kind = parse_kind(&kind)?;
    if body.token.trim().is_empty() {
        return Err(bad_request(bisa_core::text!(
            "error-node-codehost-empty-token"
        )));
    }
    let (login, connection) = eng::add_account(
        state.engine.inner(),
        kind,
        &body.token,
        body.login.as_deref(),
    )
    .await?;
    Ok(Json(json!({"login": login, "connection": connection})))
}

async fn forget_account(
    State(state): State<Shared>,
    AxPath((kind, login)): AxPath<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let kind = parse_kind(&kind)?;
    eng::forget_account(state.engine.inner(), kind, &login)?;
    Ok(Json(
        json!({"login": login.to_ascii_lowercase(), "forgotten": true}),
    ))
}

/// Set or clear a kind's default account — the global `codehost.<kind>.account`.
async fn set_default_account(
    State(state): State<Shared>,
    AxPath(kind): AxPath<String>,
    crate::Body(body): crate::Body<DefaultAccountBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let kind = parse_kind(&kind)?;
    let default =
        eng::set_default_account(state.engine.inner(), kind, body.login.as_deref()).await?;
    Ok(Json(json!({"default": default})))
}

pub const ROUTES: &[RouteDoc] = &[
    RouteDoc { method: "GET", path: "/workstreams/{wid}/pr", summary: "The workstream's pull request, read fresh from the code host; `pr: null` when none was opened." },
    RouteDoc { method: "GET", path: "/workstreams/{wid}/pr/checks", summary: "Check runs on the workstream's pull request (empty for a code host without them)." },
    RouteDoc { method: "GET", path: "/workstreams/{wid}/pr/reviews", summary: "The pull request's submitted reviews and resolvable inline threads, read from the code host (empty for a code host without the capability)." },
    RouteDoc { method: "POST", path: "/workstreams/{wid}/pr/review", summary: "Submit a review: `{event: approve | request_changes | comment, body?, comments?: [{path, line, body, side?: left | right, start_line?, start_side?}]}`. A comment or a change request needs a `body`; the events a code host takes are its `capabilities().review_events` (GitLab has no `request_changes`). The connected account may only `comment` on a pull request it opened itself: an `approve` or `request_changes` there is `409 own_pull_request` (`detail.author`) before any request to the code host." },
    RouteDoc { method: "POST", path: "/workstreams/{wid}/pr/threads/{thread_id}/resolve", summary: "Resolve or unresolve one review thread: `{resolved: bool}` (GraphQL on GitHub; a discussion on GitLab; a comment's resolution on Bitbucket)." },
    RouteDoc { method: "POST", path: "/workstreams/{wid}/pr/threads/{thread_id}/reply", summary: "Reply on one review thread as the connected account: `{body, resolve?: bool}` — the person's words untouched (an agent replies through the `pr_thread_reply` MCP tool, signed), and with `resolve: true` the thread is resolved in the same act. Empty words are a 400 before the code host is asked; a code host without `review_thread_replies` refuses. Answers `{workstream, thread, replied: true, resolved}`." },
    RouteDoc { method: "POST", path: "/workstreams/{wid}/pr/merge", summary: "Merge the pull request — through the `publish` gate: `{strategy: merge | squash | rebase, delete_branch?}`; 200 merged (with `remote_branch_deleted` when asked), 202 gate open, 409 refused. The first writer of `merged`." },
    RouteDoc { method: "GET", path: "/projects/{pid}/prs", summary: "The open pull requests on the code host behind the project's `origin` — `{project, prs: PullRequest[]}`, read fresh — what a workstream can be opened from (`POST /projects/{pid}/workstreams` with `source: pull_request`). A project with no `origin` or no known code host is the code host's 404." },
    RouteDoc { method: "GET", path: "/codehost/capabilities/{pid}", summary: "What the code host behind the project's `origin` can do — drafts, reviewers, labels, merge strategies, checks, review comments, threads, branch deletion, the review events it takes — or `code_host: null`. The pull-request form is rendered from this." },
    RouteDoc { method: "POST", path: "/codehost/inspect", summary: "What a remote is before a project exists — `{url}` for a clone or `{path}` for a folder the person picked, one of the two (ide/08 §Surfaces): the URL taken apart with its SSH alias resolved (`remote`), the code host by kind and instance (`code_host`, null for a host this build does not know by name), the profile it falls under, every account that could speak for it (`accounts[]` with `source`: `profile` · `default` · `cli` · `stored` · `git_helper`) and the one `suggested`, and the `cautions` a person would want before creating. Offline: nothing reaches the remote." },
    RouteDoc { method: "GET", path: "/codehost/{kind}/accounts", summary: "One kind's accounts (`github` · `gitlab` · `bitbucket`; ide/08 §Credentials): every stored login with where its token lives (`accounts[].source`: `file` · `keyring`), whether the kind's environment variable overrides them all (`env_override`), where a new one goes (`store`), every credential helper git is configured with (`helpers`) and the username it holds for the host (`helper_username`), the login the kind's CLI is signed in as (`cli_login`), and the `default` account — the global `codehost.<kind>.account`. Never a token; no request to the host. An unknown kind is a 404 naming the three." },
    RouteDoc { method: "PUT", path: "/codehost/{kind}/accounts", summary: "Add an account: `{token, login?}` — the token is checked with the host first (a refused token is a 401 and nothing is kept) and stored under the login the host answers, as a 0600 file under the workspace's `identity/codehost/<kind>/` or in the OS keyring when chosen; written once, never read back. Bitbucket's API token is a Basic credential, so `login` is required there. Answers `{login, connection}`." },
    RouteDoc { method: "DELETE", path: "/codehost/{kind}/accounts/{login}", summary: "Forget one account's token; a login never stored is not an error. Answers `{login, forgotten: true}`." },
    RouteDoc { method: "POST", path: "/codehost/{kind}/accounts/{login}/check", summary: "Ask the host whose `{login}`'s credential is — through the CLI when it holds that account, else the API with the stored token: `{state: connected, login, scopes, missing, recommended_missing, organizations}` (the scopes the kind needs that a token lacks; the organizations, groups or workspaces the account can see), `{state: refused, reason}`, `{state: unreachable, reason}`, or `{state: no_token}` without a request." },
    RouteDoc { method: "PUT", path: "/codehost/{kind}/default", summary: "Set the kind's default account — the global `codehost.<kind>.account`, the login a checkout with no profile and no pin uses — with `{login}`, or clear it with `{login: null}`. Answers `{default}`." },
    RouteDoc { method: "GET", path: "/codehost/{kind}/health", summary: "One kind's health for Settings (ide/08 §Connection is a request): the CLI (`cli`: installed, path, version, the accounts it is signed in as and which is active; null for Bitbucket, which has none), the stored accounts, git's helpers and username, the environment override, the `default`, who would answer with nothing named (`resolves`: `{login, source}`), and the host's `token_page`. Offline — the CLI's `auth status` is the one program asked, time-boxed. Never a token." },
    RouteDoc { method: "GET", path: "/codehost/{kind}/login", summary: "How to sign in to one kind: `{kind: cli, program, host, words, token_page}` when the CLI is installed (the desktop shell runs its browser login in a terminal), `{kind: install, program, hints: {brew, apt, winget, url}, words, token_page}` when it is not, `{kind: token, token_page, words}` for a kind with no CLI." },
];
