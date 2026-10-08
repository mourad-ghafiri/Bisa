//! Connectors over HTTP: the definitions this workspace holds (the catalog's
//! and a person's own), the **accounts** of each on this machine — their
//! labels, parameters, which secret fields are set and where they live, never
//! a value — one live *check* per account, and the OAuth flow: `oauth/start`
//! answers the authorization URL and brings up the loopback listener the
//! browser comes back to; `oauth/callback` (token-exempt, on that listener and
//! on this one) finishes the exchange and shows a page with no script in it;
//! `oauth/complete` takes a pasted code for a platform that cannot redirect.
//!
//! Every write goes through `bisa_engine::connectors` (rule 2); a
//! definition is validated before it is recorded and the problems come back
//! typed; a catalog definition is the catalog's and is never edited here.

use crate::dto::{
    CheckAccountBody, ConnectorAccountRow, ConnectorDefinitionBody, ConnectorDetailDto,
    ConnectorProblemDto, ConnectorRow, ConnectorValidationDto, NewConnectorAccountBody,
    OauthCompleteBody,
};
use crate::route_docs::RouteDoc;
use crate::Query;
use crate::{ApiError, Shared};
use axum::extract::{Path as AxPath, State};
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use bisa_core::Localize as _;
use bisa_core::{AccountId, Connector, ConnectorAccount, ConnectorId};
use bisa_engine::connectors as eng;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

pub(crate) fn routes() -> Router<Shared> {
    Router::new()
        .route("/connectors", get(list).post(create))
        .route("/connectors/validate", post(validate))
        .route("/connectors/oauth/callback", get(oauth_callback))
        .route("/connectors/{cid}", get(detail).put(update).delete(remove))
        .route("/connectors/{cid}/accounts", get(accounts).put(put_account))
        .route("/connectors/{cid}/accounts/{aid}", delete(delete_account))
        .route(
            "/connectors/{cid}/accounts/{aid}/check",
            post(check_account),
        )
        .route(
            "/connectors/{cid}/accounts/{aid}/default",
            put(make_default),
        )
        .route(
            "/connectors/{cid}/accounts/{aid}/oauth/start",
            post(oauth_start),
        )
        .route(
            "/connectors/{cid}/accounts/{aid}/oauth/complete",
            post(oauth_complete),
        )
}

/// The one path the browser is sent back to, on the callback listener and
/// on the main one.
pub const OAUTH_CALLBACK_PATH: &str = "/connectors/oauth/callback";

// ---------------------------------------------------------------------------
// Parsing and shaping
// ---------------------------------------------------------------------------

fn parse_connector_id(raw: &str) -> Result<ConnectorId, ApiError> {
    ConnectorId::new(raw).map_err(|e| ApiError::text(StatusCode::NOT_FOUND, e.text()))
}

fn parse_account_id(raw: &str) -> Result<AccountId, ApiError> {
    raw.parse::<AccountId>().map_err(|_| {
        ApiError::text(
            StatusCode::NOT_FOUND,
            bisa_core::text!(
                "error-node-connectors-not-account-id",
                raw = format!("{raw:?}")
            ),
        )
    })
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// The connector, or a 404 that names it.
fn connector_of(state: &Shared, cid: &ConnectorId) -> Result<Connector, ApiError> {
    state.engine.workspace().get_connector(cid).map_err(|e| {
        ApiError::text(
            StatusCode::NOT_FOUND,
            bisa_core::text!("error-node-connectors-refused", detail = e.to_string()),
        )
    })
}

fn account_rows(state: &Shared, def: &Connector) -> Result<Vec<ConnectorAccountRow>, ApiError> {
    let ws = state.engine.workspace();
    let now = now_secs();
    let mut out = Vec::new();
    for a in ws.list_connector_accounts(&def.id)? {
        let facts = ws.connector_account_secrets(&def.id, a.id)?;
        let health = state.engine.inner().connector_health.view_of(&def.id, a.id);
        out.push(ConnectorAccountRow::from_parts(
            &a,
            &def.auth,
            facts.source,
            now,
            health,
        ));
    }
    Ok(out)
}

fn account_row(
    state: &Shared,
    def: &Connector,
    a: &ConnectorAccount,
) -> Result<ConnectorAccountRow, ApiError> {
    let facts = state
        .engine
        .workspace()
        .connector_account_secrets(&def.id, a.id)?;
    let health = state.engine.inner().connector_health.view_of(&def.id, a.id);
    Ok(ConnectorAccountRow::from_parts(
        a,
        &def.auth,
        facts.source,
        now_secs(),
        health,
    ))
}

/// A definition that does not validate: 400, the problems typed in `detail`.
fn refuse_problems(problems: &[bisa_core::ConnectorProblem]) -> ApiError {
    let rows: Vec<ConnectorProblemDto> = problems.iter().map(ConnectorProblemDto::from).collect();
    let words = problems
        .iter()
        .take(3)
        .map(|p| match &p.field {
            Some(f) => format!("{f}: {}", p.text),
            None => p.text.to_string(),
        })
        .collect::<Vec<_>>()
        .join("; ");
    let more = problems.len().saturating_sub(3);
    ApiError::text(
        StatusCode::BAD_REQUEST,
        bisa_core::text!(
            "error-node-connectors-connector-definition-has-problem",
            a0 = (problems.len()).to_string(),
            a1 = (if problems.len() == 1 { "" } else { "s" }).to_string(),
            words = words.to_string(),
            a2 = (if more > 0 {
                format!(" (and {more} more)")
            } else {
                String::new()
            })
            .to_string()
        ),
    )
    .with_detail(json!({ "problems": rows }))
}

// ---------------------------------------------------------------------------
// Definitions
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub(crate) struct ListQuery {
    #[serde(default)]
    tag: Option<String>,
}

async fn list(
    State(state): State<Shared>,
    Query(q): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let ws = state.engine.workspace();
    let mut rows = Vec::new();
    for c in ws.list_connectors()? {
        if let Some(tag) = &q.tag {
            if !c.tags.contains(tag) {
                continue;
            }
        }
        let accounts = ws.list_connector_accounts(&c.id)?;
        rows.push(ConnectorRow::from_parts(&c, &accounts));
    }
    Ok(Json(json!({ "connectors": rows })))
}

async fn create(
    State(state): State<Shared>,
    crate::Body(body): crate::Body<ConnectorDefinitionBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let draft = body.as_connector().map_err(|e| {
        ApiError::text(
            StatusCode::BAD_REQUEST,
            bisa_core::text!("error-node-connectors-refused", detail = e.to_string()),
        )
    })?;
    let problems = draft.validate();
    if !problems.is_empty() {
        return Err(refuse_problems(&problems));
    }
    let new = body.into_new().map_err(|e| {
        ApiError::text(
            StatusCode::BAD_REQUEST,
            bisa_core::text!("error-node-connectors-refused", detail = e.to_string()),
        )
    })?;
    let created = eng::create_connector(state.engine.inner(), new)?;
    Ok(Json(
        json!({ "connector": ConnectorRow::from_parts(&created, &[]) }),
    ))
}

async fn validate(
    crate::Body(body): crate::Body<ConnectorDefinitionBody>,
) -> Result<Json<ConnectorValidationDto>, ApiError> {
    let problems = match body.as_connector() {
        Ok(c) => c
            .validate()
            .iter()
            .map(ConnectorProblemDto::from)
            .collect::<Vec<_>>(),
        Err(e) => vec![ConnectorProblemDto {
            field: Some("tags".into()),
            text: e.text(),
        }],
    };
    Ok(Json(ConnectorValidationDto {
        ok: problems.is_empty(),
        problems,
    }))
}

async fn detail(
    State(state): State<Shared>,
    AxPath(cid): AxPath<String>,
) -> Result<Json<ConnectorDetailDto>, ApiError> {
    let cid = parse_connector_id(&cid)?;
    let def = connector_of(&state, &cid)?;
    let accounts = account_rows(&state, &def)?;
    Ok(Json(ConnectorDetailDto {
        connector: def,
        accounts,
    }))
}

async fn update(
    State(state): State<Shared>,
    AxPath(cid): AxPath<String>,
    crate::Body(body): crate::Body<ConnectorDefinitionBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let cid = parse_connector_id(&cid)?;
    let existing = connector_of(&state, &cid)?;
    if matches!(existing.origin, bisa_core::Origin::Catalog { .. }) {
        return Err(ApiError::text(
            StatusCode::CONFLICT,
            bisa_core::text!(
                "error-node-connectors-connector-catalog-s-not-edited-write-your",
                cid = cid.to_string()
            ),
        ));
    }
    if body.id != cid {
        return Err(ApiError::text(
            StatusCode::BAD_REQUEST,
            bisa_core::text!(
                "error-node-connectors-body-names-connector-but-path-names",
                a0 = (body.id).to_string(),
                cid = cid.to_string()
            ),
        ));
    }
    let mut draft = body.as_connector().map_err(|e| {
        ApiError::text(
            StatusCode::BAD_REQUEST,
            bisa_core::text!("error-node-connectors-refused", detail = e.to_string()),
        )
    })?;
    let problems = draft.validate();
    if !problems.is_empty() {
        return Err(refuse_problems(&problems));
    }
    draft.origin = existing.origin.clone();
    draft.created_at = existing.created_at;
    let updated = eng::update_connector(state.engine.inner(), draft)?;
    let accounts = state.engine.workspace().list_connector_accounts(&cid)?;
    Ok(Json(
        json!({ "connector": ConnectorRow::from_parts(&updated, &accounts) }),
    ))
}

async fn remove(
    State(state): State<Shared>,
    AxPath(cid): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let cid = parse_connector_id(&cid)?;
    connector_of(&state, &cid)?;
    let usage = state
        .engine
        .workspace()
        .usage_of(bisa_store::UsageKind::Connector, cid.as_str())?;
    if !usage.is_empty() {
        return Err(ApiError::text(
            StatusCode::CONFLICT,
            bisa_core::text!(
                "error-node-connectors-cannot-remove-connector-still-used-forget-accounts",
                cid = cid.to_string(),
                a0 = (usage.describe()).to_string()
            ),
        )
        .with_detail(json!({ "usage": usage.as_slice() })));
    }
    eng::remove_connector(state.engine.inner(), &cid)?;
    Ok(Json(
        json!({ "connector": cid.to_string(), "removed": true }),
    ))
}

// ---------------------------------------------------------------------------
// Accounts
// ---------------------------------------------------------------------------

async fn accounts(
    State(state): State<Shared>,
    AxPath(cid): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let cid = parse_connector_id(&cid)?;
    let def = connector_of(&state, &cid)?;
    Ok(Json(json!({ "accounts": account_rows(&state, &def)? })))
}

async fn put_account(
    State(state): State<Shared>,
    AxPath(cid): AxPath<String>,
    crate::Body(body): crate::Body<NewConnectorAccountBody>,
) -> Result<Json<ConnectorAccountRow>, ApiError> {
    let cid = parse_connector_id(&cid)?;
    let def = connector_of(&state, &cid)?;
    for field in body.secrets.keys() {
        if !def.auth.fields().contains(field) {
            return Err(ApiError::text(
                StatusCode::BAD_REQUEST,
                bisa_core::text!(
                    "error-node-connectors-connector-authenticates-with-has-no-secret-field",
                    cid = cid.to_string(),
                    a0 = (def.auth.word()).to_string(),
                    a1 = (field.as_str()).to_string()
                ),
            ));
        }
    }
    let inner = state.engine.inner();
    let account = match body.id {
        None => eng::put_account(
            inner,
            bisa_store::NewConnectorAccount {
                connector: cid.clone(),
                label: body.label,
                params: body.params,
                default: body.default.unwrap_or(false),
            },
            body.secrets,
        )?,
        Some(aid) => {
            let mut a = eng::update_account(inner, &cid, aid, body.label, body.params)?;
            if !body.secrets.is_empty() {
                a = eng::set_secrets(inner, &cid, aid, body.secrets)?;
            }
            if body.default == Some(true) && !a.default {
                a = eng::set_default_account(inner, &cid, aid)?;
            }
            a
        }
    };
    Ok(Json(account_row(&state, &def, &account)?))
}

async fn delete_account(
    State(state): State<Shared>,
    AxPath((cid, aid)): AxPath<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let cid = parse_connector_id(&cid)?;
    let aid = parse_account_id(&aid)?;
    eng::delete_account(state.engine.inner(), &cid, aid)?;
    Ok(Json(
        json!({ "account": aid.to_string(), "forgotten": true }),
    ))
}

async fn check_account(
    State(state): State<Shared>,
    AxPath((cid, aid)): AxPath<(String, String)>,
    body: Option<crate::Body<CheckAccountBody>>,
) -> Result<Json<eng::AccountCheck>, ApiError> {
    let cid = parse_connector_id(&cid)?;
    let aid = parse_account_id(&aid)?;
    let budget = body.and_then(|crate::Body(b)| b.timeout_secs);
    let inner = state.engine.inner();
    Ok(Json(
        inner
            .connector_health
            .check(inner, &cid, aid, budget)
            .await?,
    ))
}

async fn make_default(
    State(state): State<Shared>,
    AxPath((cid, aid)): AxPath<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let cid = parse_connector_id(&cid)?;
    let aid = parse_account_id(&aid)?;
    eng::set_default_account(state.engine.inner(), &cid, aid)?;
    Ok(Json(json!({ "default": aid.to_string() })))
}

// ---------------------------------------------------------------------------
// OAuth
// ---------------------------------------------------------------------------

/// The loopback port the browser is sent back to — the machine setting.
fn oauth_port(state: &Shared) -> Result<u16, ApiError> {
    let resolved = state
        .engine
        .workspace()
        .setting("connectors.oauth.port", None)?;
    resolved
        .value
        .as_u64()
        .and_then(|p| u16::try_from(p).ok())
        .ok_or_else(|| {
            ApiError::text(
                StatusCode::INTERNAL_SERVER_ERROR,
                bisa_core::text!("error-node-connectors-connectors-oauth-port-not-port-number"),
            )
        })
}

async fn oauth_start(
    State(state): State<Shared>,
    AxPath((cid, aid)): AxPath<(String, String)>,
) -> Result<Json<eng::OAuthStart>, ApiError> {
    let cid = parse_connector_id(&cid)?;
    let aid = parse_account_id(&aid)?;
    let port = oauth_port(&state)?;
    // The listener first: a busy port is refused before a flow is pending.
    state.oauth.ensure_started(Arc::clone(&state), port).await?;
    let started = eng::oauth_start(state.engine.inner(), &cid, aid, port)?;
    Ok(Json(started))
}

async fn oauth_complete(
    State(state): State<Shared>,
    AxPath((cid, aid)): AxPath<(String, String)>,
    crate::Body(body): crate::Body<OauthCompleteBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let cid = parse_connector_id(&cid)?;
    let aid = parse_account_id(&aid)?;
    if body.code.trim().is_empty() {
        return Err(ApiError::text(
            StatusCode::BAD_REQUEST,
            bisa_core::text!("error-node-connectors-code-empty"),
        ));
    }
    eng::oauth_paste(state.engine.inner(), &cid, aid, body.code.trim()).await?;
    state.oauth.stop_if_idle(&state).await;
    Ok(Json(
        json!({ "account": aid.to_string(), "connected": true }),
    ))
}

#[derive(Deserialize, Default)]
pub(crate) struct CallbackQuery {
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    error_description: Option<String>,
}

fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// The page the browser lands on. No script, no network: the policy says
/// so, and the code is never echoed.
fn page(status: StatusCode, title: &str, body: &str) -> Response {
    let html = format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>{title}</title>\
         <style>body{{font:16px/1.5 system-ui,sans-serif;max-width:32rem;margin:4rem auto;padding:0 1rem;color:#222}}\
         h1{{font-size:1.25rem}}</style></head><body><h1>{title}</h1><p>{body}</p></body></html>"
    );
    (
        status,
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (
                header::CONTENT_SECURITY_POLICY,
                "default-src 'none'; style-src 'unsafe-inline'",
            ),
        ],
        Html(html),
    )
        .into_response()
}

async fn oauth_callback(
    State(state): State<Shared>,
    crate::i18n::Lang(locale): crate::i18n::Lang,
    Query(q): Query<CallbackQuery>,
) -> Response {
    // The page a person reads in their browser after the platform sent them back: said in the request's language.
    let word = |text: &bisa_core::Text| bisa_i18n::render(&locale, text);
    if let Some(error) = q.error {
        let why = q
            .error_description
            .filter(|d| !d.trim().is_empty())
            .unwrap_or(error);
        state.oauth.stop_if_idle(&state).await;
        return page(
            StatusCode::BAD_REQUEST,
            &word(&bisa_core::text!(
                "error-node-connectors-page-not-connected"
            )),
            &word(&bisa_core::text!(
                "error-node-connectors-page-declined",
                why = escape_html(&why)
            )),
        );
    }
    let (Some(flow_state), Some(code)) = (q.state, q.code) else {
        return page(
            StatusCode::BAD_REQUEST,
            &word(&bisa_core::text!(
                "error-node-connectors-page-not-connected"
            )),
            &word(&bisa_core::text!("error-node-connectors-page-stale-link")),
        );
    };
    let done = eng::oauth_complete(state.engine.inner(), flow_state.trim(), code.trim()).await;
    state.oauth.stop_if_idle(&state).await;
    match done {
        Ok(_) => page(
            StatusCode::OK,
            &word(&bisa_core::text!("error-node-connectors-page-connected")),
            &word(&bisa_core::text!(
                "error-node-connectors-page-connected-close-window"
            )),
        ),
        Err(e) if e.is_refusal() => page(
            StatusCode::BAD_REQUEST,
            &word(&bisa_core::text!(
                "error-node-connectors-page-not-connected"
            )),
            &word(&bisa_core::text!("error-node-connectors-page-stale-link")),
        ),
        Err(e) => {
            // Which connector and account it was is the engine's line to say.
            tracing::warn!(target: "bisa_node::connectors", "the OAuth callback did not finish the connection: {e}");
            page(
                StatusCode::BAD_GATEWAY,
                &word(&bisa_core::text!(
                    "error-node-connectors-page-not-connected"
                )),
                &word(&bisa_core::text!("error-node-connectors-page-unreachable")),
            )
        }
    }
}

// ---------------------------------------------------------------------------
// The callback listener
// ---------------------------------------------------------------------------

struct Live {
    stop: tokio::sync::watch::Sender<bool>,
    task: tokio::task::JoinHandle<()>,
    port: u16,
}

/// The loopback listener the browser is sent back to, up only while a flow
/// is pending: bound on `oauth/start`, stopped when the last flow completes,
/// after the flows' lifetime has passed, or with the node.
#[derive(Default)]
pub(crate) struct CallbackListener {
    live: tokio::sync::Mutex<Option<Live>>,
}

impl CallbackListener {
    /// Bind the port, or say why not. Idempotent while up on the same port.
    pub(crate) async fn ensure_started(&self, state: Shared, port: u16) -> Result<(), ApiError> {
        let mut guard = self.live.lock().await;
        if let Some(live) = guard.as_ref() {
            if !live.task.is_finished() && live.port == port {
                return Ok(());
            }
            // A different port, or a listener that ended: start afresh. A
            // stop nobody hears is a listener already gone.
            let _listener_gone = live.stop.send(true);
        }
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
            .await
            .map_err(|e| {
                ApiError::text(
                    StatusCode::CONFLICT,
                    bisa_core::text!(
                        "error-node-connectors-port-busy-change-connectors-oauth-port",
                        port = port.to_string(),
                        e = e.to_string()
                    ),
                )
            })?;
        let (stop, rx) = tokio::sync::watch::channel(false);
        let router = Router::new()
            .route(OAUTH_CALLBACK_PATH, get(oauth_callback))
            .with_state(Arc::clone(&state));
        let watchdog_stop = stop.clone();
        let watchdog_state = Arc::clone(&state);
        let task = tokio::spawn(async move {
            let serve = axum::serve(listener, router).with_graceful_shutdown(crate::stopped(rx));
            let deadline = Duration::from_secs(eng::PENDING_TTL_SECS + 30);
            let watchdog = async move {
                let started = tokio::time::Instant::now();
                loop {
                    tokio::time::sleep(Duration::from_secs(10)).await;
                    let idle = eng::oauth_pending(watchdog_state.engine.inner()) == 0;
                    if (idle && started.elapsed() >= Duration::from_secs(30))
                        || started.elapsed() >= deadline
                    {
                        let _listener_gone = watchdog_stop.send(true);
                        break;
                    }
                }
            };
            tokio::select! {
                _ = serve => {}
                _ = watchdog => {}
            }
        });
        tracing::info!("oauth callback listener on http://127.0.0.1:{port}{OAUTH_CALLBACK_PATH}");
        *guard = Some(Live { stop, task, port });
        Ok(())
    }

    /// Stop the listener when no flow is waiting for a browser any more.
    pub(crate) async fn stop_if_idle(&self, state: &Shared) {
        if eng::oauth_pending(state.engine.inner()) == 0 {
            self.stop().await;
        }
    }

    pub(crate) async fn stop(&self) {
        let mut guard = self.live.lock().await;
        if let Some(live) = guard.take() {
            // The listener may already be gone; a stop nobody hears is fine.
            let _listener_gone = live.stop.send(true);
            if tokio::time::timeout(Duration::from_secs(5), live.task)
                .await
                .is_err()
            {
                tracing::warn!(
                    target: "bisa_node",
                    "the OAuth callback listener did not stop within five seconds; abandoned"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The reference
// ---------------------------------------------------------------------------

pub const ROUTES: &[RouteDoc] = &[
    RouteDoc {
        method: "GET",
        path: "/connectors",
        summary: "Every connector this workspace holds — the catalog's and your own — as `{connectors: ConnectorRow[]}`: auth scheme, hosts, operations with their parameters, the `check` operation, how many accounts this machine has and which is the default. `?tag=` narrows.",
    },
    RouteDoc {
        method: "POST",
        path: "/connectors",
        summary: "Record your own connector definition (`ConnectorDefinition`: id, name, description, tags, base_url, hosts, auth, params, operations, check). Validated first: a 400 carries every problem typed under `detail.problems`.",
    },
    RouteDoc {
        method: "POST",
        path: "/connectors/validate",
        summary: "Validate a definition without recording it: `{ok, problems: [{field?, message}]}`.",
    },
    RouteDoc {
        method: "GET",
        path: "/connectors/{cid}",
        summary: "One definition whole, and this machine's accounts for it (`ConnectorDetail`) — which secret fields each holds and where they live, never a value.",
    },
    RouteDoc {
        method: "PUT",
        path: "/connectors/{cid}",
        summary: "Replace your own definition (validated like a create). A catalog connector is 409: write your own under another id.",
    },
    RouteDoc {
        method: "DELETE",
        path: "/connectors/{cid}",
        summary: "Remove a definition. 409 while an account or a workflow step still names it, naming them.",
    },
    RouteDoc {
        method: "GET",
        path: "/connectors/{cid}/accounts",
        summary: "This machine's accounts for the connector: `{accounts: ConnectorAccountRow[]}` — label, parameters, the default mark, the secret fields set and their source (`file` or `keyring`), an OAuth account's expiry, and `health`: what the last check found (`unknown | ok | failing`, when, the status and the reason), kept for the engine's lifetime and forgotten when the way in changes. Never a token.",
    },
    RouteDoc {
        method: "PUT",
        path: "/connectors/{cid}/accounts",
        summary: "Add an account (`{label, params?, secrets?, default?}`) or edit one (`{id, …}`). `secrets` is a map of field to value written once — each field replaced, none read back; a field the scheme does not have is 400. Answers the account row.",
    },
    RouteDoc {
        method: "DELETE",
        path: "/connectors/{cid}/accounts/{aid}",
        summary: "Forget an account and every secret it held: `{account, forgotten: true}`.",
    },
    RouteDoc {
        method: "POST",
        path: "/connectors/{cid}/accounts/{aid}/check",
        summary: "Run the connector's `check` operation as the account — one live request within `{timeout_secs?}` (1–60, the engine's 20 unsaid; the body may be left out): `{state: connected | refused | unreachable | no_check, status?, reason?}`, the reason redacted. The answer is kept as the account's `health`, a check already running for the account is joined, and the bus hears `connectors.checked`.",
    },
    RouteDoc {
        method: "PUT",
        path: "/connectors/{cid}/accounts/{aid}/default",
        summary: "Make this the connector's default account — the one a step with no account named runs as: `{default}`.",
    },
    RouteDoc {
        method: "POST",
        path: "/connectors/{cid}/accounts/{aid}/oauth/start",
        summary: "Begin connecting an OAuth account with the person's own client id: binds the loopback callback listener on `connectors.oauth.port` (409 when the port is busy) and answers `{url, redirect_uri, expires_at}` — open `url` in the browser.",
    },
    RouteDoc {
        method: "GET",
        path: "/connectors/oauth/callback",
        summary: "Where the browser comes back (`?state&code`, or `?error`). Answers a page with no script; the code is exchanged and never echoed; a stale or unknown state is 400. Token-exempt, on the callback listener and here.",
    },
    RouteDoc {
        method: "POST",
        path: "/connectors/{cid}/accounts/{aid}/oauth/complete",
        summary: "Finish a started connection with a code the person pasted (`{code}`), for a platform that cannot send the browser back to loopback.",
    },
];
