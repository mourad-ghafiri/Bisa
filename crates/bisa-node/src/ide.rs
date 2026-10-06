//! The IDE's file routes (ide/03): read with the editor's cap, save with
//! compare-and-swap, create, move, delete, and the watcher lease.
//!
//! Thin by construction: every decision — the writable root, the containment
//! check, the hash comparison, what a delete may remove — is
//! `bisa_engine::ide::files`. The one thing this module decides is the
//! shape of a `409`: it carries the current text, because a sentence is not
//! something a client can merge from.

use crate::dto::{
    IdeCopyBody, IdeCreateBody, IdeDeleteBody, IdeFileDto, IdeMoveBody, IdeWriteBody,
};
use crate::route_docs::RouteDoc;
use crate::Query;
use crate::{bad_request, ApiError, Shared};
use axum::extract::{DefaultBodyLimit, Path as AxPath, State};
use axum::http::{header, StatusCode};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use bisa_core::Localize;
use bisa_engine::ide::files::{self, Disposal, EntryKind};
use bisa_engine::ide::graph;
use bisa_engine::ide::index;
use bisa_engine::ide::layout;
use bisa_engine::ide::search::{self, CaseMode, SearchQuery};
use bisa_engine::ide::watch;
use bisa_engine::EngineError;
use bisa_store::{FileScope, ReadCap};
use serde::Deserialize;
use serde_json::json;
use std::str::FromStr;
use std::sync::Arc;

pub mod consent;
pub mod interactive;
pub mod lsp;
pub mod serve;

/// The editor's read policy as nobody set it: editable to here, read-only
/// to the refusal. This machine moves both (`files::EditorCaps`,
/// `editor.large_file.*`); the desktop's `editorModel.mjs` carries the first
/// — above it a file is drawn plain — held equal by its parity test.
pub const EDITABLE_BYTES: u64 = files::EditorCaps::UNSET.editable;
pub const REFUSE_BYTES: u64 = files::EditorCaps::UNSET.refuse;
/// The most a save may carry: the editable text as a JSON string — every
/// byte of it may need an escape, so twice the text — and its base hash.
/// Sized from the most *Editable up to* may be set to, so "editable" is a
/// promise the write side keeps whatever this machine set; the text itself
/// is held to the bound in force by `write`.
pub const WRITE_BODY_BYTES: usize = 2 * files::EditorCaps::MOST_EDITABLE as usize + 64 * 1024;

pub(crate) fn routes() -> Router<Shared> {
    Router::new()
        .route(
            "/ide/file/{scope}/{id}",
            get(read)
                .put(write)
                .layer(DefaultBodyLimit::max(WRITE_BODY_BYTES)),
        )
        .route("/ide/raw/{scope}/{id}", get(raw))
        .route("/ide/files/{scope}/{id}", post(create).delete(delete))
        .route("/ide/files/{scope}/{id}/delete", post(delete_many))
        .route("/ide/files/{scope}/{id}/move", post(move_entry))
        .route("/ide/files/{scope}/{id}/copy", post(copy_entry))
        .route("/ide/files/{scope}/{id}/disposal", get(disposal))
        .route(
            "/ide/watch/{scope}/{id}",
            post(watch_root).delete(unwatch_root),
        )
        .route("/ide/search/{scope}/{id}", get(search_stream))
        .route("/ide/replace/{scope}/{id}", post(replace))
        .route(
            "/ide/layout/{scope}/{id}",
            get(layout_read).put(layout_write),
        )
        .route("/ide/index/{scope}/{id}", get(path_index))
        .route("/ide/graph/{scope}/{id}", get(graph_window))
        .route("/ide/graph/{scope}/{id}/search", get(graph_search))
}

#[derive(Deserialize)]
struct GraphQuery {
    #[serde(default)]
    from: Option<usize>,
    #[serde(default)]
    count: Option<usize>,
    /// Relay the graph even if no ref moved.
    #[serde(default)]
    refresh: bool,
    /// `all` (every local branch and tag — the default) or `head`.
    #[serde(default)]
    refs: Option<String>,
}

/// The ref scope a query names, `all` when it names none; anything else is a 400.
fn ref_scope(refs: Option<&str>) -> Result<graph::RefScope, ApiError> {
    match refs {
        None => Ok(graph::RefScope::All),
        Some(s) => s.parse::<graph::RefScope>().map_err(|e| {
            bad_request(bisa_core::text!(
                "error-node-ide-not-ref-scope",
                e = e.to_string()
            ))
        }),
    }
}

/// A window of the commit graph (ide/05): rows `from..from+count` with their
/// lanes and edges, `total` and whether the whole log is laid out yet.
async fn graph_window(
    State(state): State<Shared>,
    AxPath((scope, id)): AxPath<(String, String)>,
    Query(q): Query<GraphQuery>,
) -> Result<Json<graph::Window>, ApiError> {
    let scope = parse_scope(&scope)?;
    let refs = ref_scope(q.refs.as_deref())?;
    let w = graph::window(
        state.engine.inner(),
        scope,
        &id,
        refs,
        q.from.unwrap_or(0),
        q.count.unwrap_or(400),
        q.refresh,
    )
    .await?;
    Ok(Json(w))
}

#[derive(Deserialize)]
struct GraphSearchQuery {
    #[serde(default)]
    q: String,
    #[serde(default)]
    from: Option<usize>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    refs: Option<String>,
}

/// Search the whole laid-out log (ide/05): the row indexes matching `q` on
/// subject, author, id prefix or ref name, so the client can jump to a
/// commit far past the page it has loaded.
async fn graph_search(
    State(state): State<Shared>,
    AxPath((scope, id)): AxPath<(String, String)>,
    Query(q): Query<GraphSearchQuery>,
) -> Result<Json<graph::Matches>, ApiError> {
    let scope = parse_scope(&scope)?;
    if q.q.trim().is_empty() {
        return Err(bad_request(bisa_core::text!("error-node-ide-q-required")));
    }
    let refs = ref_scope(q.refs.as_deref())?;
    let m = graph::search(
        state.engine.inner(),
        scope,
        &id,
        refs,
        &q.q,
        q.from.unwrap_or(0),
        q.limit.unwrap_or(500),
    )
    .await?;
    Ok(Json(m))
}

#[derive(Deserialize)]
struct IndexQuery {
    #[serde(default)]
    limit: Option<usize>,
}

/// Filesystem work runs on the blocking pool: a walk of a large tree, a
/// compare-and-swap write, a trash call are not things a runtime thread
/// waits on while every other request queues behind them.
async fn blocking<T, F>(f: F) -> Result<T, ApiError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, EngineError> + Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| ApiError::internal(&e))?
        .map_err(ApiError::from)
}

async fn path_index(
    State(state): State<Shared>,
    AxPath((scope, id)): AxPath<(String, String)>,
    Query(q): Query<IndexQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let scope = parse_scope(&scope)?;
    let inner = Arc::clone(state.engine.inner());
    let idx = tokio::task::spawn_blocking(move || index::paths(&inner, scope, &id, q.limit))
        .await
        .map_err(|e| ApiError::internal(&e))??;
    Ok(Json(
        json!({"paths": idx.paths, "truncated": idx.truncated}),
    ))
}

async fn layout_read(
    State(state): State<Shared>,
    AxPath((scope, id)): AxPath<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let scope = parse_scope(&scope)?;
    let inner = Arc::clone(state.engine.inner());
    let layout = blocking(move || layout::read(&inner, scope, &id)).await?;
    Ok(Json(json!({"layout": layout})))
}

async fn layout_write(
    State(state): State<Shared>,
    AxPath((scope, id)): AxPath<(String, String)>,
    crate::Body(body): crate::Body<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let scope = parse_scope(&scope)?;
    let inner = Arc::clone(state.engine.inner());
    blocking(move || layout::write(&inner, scope, &id, &body)).await?;
    Ok(Json(json!({"ok": true})))
}

/// `?q=&regex=&case=&word=&include=&exclude=&limit=` — the search as a query
/// string, because a search is a place the URL can hold.
#[derive(Deserialize)]
struct SearchParams {
    q: String,
    #[serde(default)]
    regex: bool,
    #[serde(default)]
    case: Option<String>,
    #[serde(default)]
    word: bool,
    #[serde(default)]
    include: Option<String>,
    #[serde(default)]
    exclude: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

fn parse_case(raw: Option<&str>) -> Result<CaseMode, ApiError> {
    match raw.unwrap_or("smart") {
        "smart" => Ok(CaseMode::Smart),
        "sensitive" => Ok(CaseMode::Sensitive),
        "insensitive" => Ok(CaseMode::Insensitive),
        other => Err(bad_request(bisa_core::text!(
            "error-node-ide-case-smart-sensitive-insensitive",
            other = format!("{other:?}")
        ))),
    }
}

fn globs(raw: Option<&str>) -> Vec<String> {
    raw.map(|s| {
        s.split(',')
            .map(str::trim)
            .filter(|g| !g.is_empty())
            .map(str::to_string)
            .collect()
    })
    .unwrap_or_default()
}

impl SearchParams {
    fn query(&self) -> Result<SearchQuery, ApiError> {
        Ok(SearchQuery {
            q: self.q.clone(),
            regex: self.regex,
            case: parse_case(self.case.as_deref())?,
            word: self.word,
            include: globs(self.include.as_deref()),
            exclude: globs(self.exclude.as_deref()),
            limit: self.limit,
        })
    }
}

/// Rows stream as they are found: `{type: "hit", …}` per match, then one
/// `{type: "done", matches, files_with_matches, files_scanned, truncated}`, or
/// `{type: "error", error}` if the query was refused.
async fn search_stream(
    State(state): State<Shared>,
    AxPath((scope, id)): AxPath<(String, String)>,
    Query(params): Query<SearchParams>,
) -> Result<Sse<impl futures::Stream<Item = Result<SseEvent, std::convert::Infallible>>>, ApiError>
{
    let scope = parse_scope(&scope)?;
    let query = params.query()?;
    // A pattern that does not compile is refused here, as a 400 — never
    // reported as an error frame inside a stream that already said 200.
    search::regex_for(&query)?;
    let (tx, rx) = tokio::sync::mpsc::channel::<serde_json::Value>(256);
    let inner = Arc::clone(state.engine.inner());
    tokio::task::spawn_blocking(move || {
        let sender = tx.clone();
        let result = search::search(&inner, scope, &id, &query, |hit| {
            sender
                .blocking_send(json!({"type": "hit", "hit": hit}))
                .is_ok()
        });
        let last = match result {
            Ok(summary) => json!({
                "type": "done",
                "matches": summary.matches,
                "files_with_matches": summary.files_with_matches,
                "files_scanned": summary.files_scanned,
                "truncated": summary.truncated,
            }),
            Err(e) => {
                // Said on the stream, and in the log: the reader may be gone by now.
                tracing::warn!(target: "bisa_node", scope = scope.as_str(), %id, "a search ended in an error: {e}");
                json!({"type": "error", "error": e.to_string()})
            }
        };
        // A reader that left — the next keystroke cancelled this search — is
        // not a failure: the walk stopped at its next hit, and nobody waits
        // for the last word.
        if tx.blocking_send(last).is_err() {
            tracing::debug!(target: "bisa_node", scope = scope.as_str(), %id, "a search's reader left before it ended");
        }
    });
    use futures::StreamExt as _;
    let stream = tokio_stream::wrappers::ReceiverStream::new(rx)
        .map(|v| Ok(SseEvent::default().data(v.to_string())));
    // A search still walking when the node is asked to stop ends there: its
    // walk stops at its next hit, as it does when its reader leaves.
    Ok(Sse::new(crate::until_stopped(&state, stream)).keep_alive(KeepAlive::default()))
}

async fn replace(
    State(state): State<Shared>,
    AxPath((scope, id)): AxPath<(String, String)>,
    crate::Body(body): crate::Body<crate::dto::ReplaceBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let scope = parse_scope(&scope)?;
    let query = SearchQuery {
        q: body.q,
        regex: body.regex,
        case: parse_case(body.case.as_deref())?,
        word: body.word,
        include: body.include,
        exclude: body.exclude,
        limit: None,
    };
    let inner = Arc::clone(state.engine.inner());
    if body.apply {
        let files: Vec<(String, String)> = body
            .files
            .into_iter()
            .map(|f| (f.path, f.base_hash))
            .collect();
        if files.is_empty() {
            return Err(bad_request(bisa_core::text!(
                "error-node-ide-apply-needs-files-from-preview-each-with"
            )));
        }
        let replacement = body.replacement;
        let outcomes = blocking(move || {
            search::apply_replace(&inner, scope, &id, &query, &replacement, &files)
        })
        .await?;
        Ok(Json(json!({"applied": true, "files": outcomes})))
    } else {
        let replacement = body.replacement;
        let previews =
            blocking(move || search::preview_replace(&inner, scope, &id, &query, &replacement))
                .await?;
        Ok(Json(json!({"applied": false, "files": previews})))
    }
}

fn parse_scope(raw: &str) -> Result<FileScope, ApiError> {
    FileScope::from_str(raw).map_err(|_| {
        let valid: Vec<&str> = FileScope::ALL.iter().map(|s| s.as_str()).collect();
        bad_request(bisa_core::text!(
            "error-node-files-unknown-file-scope-use-one",
            raw = format!("{raw:?}"),
            a0 = (valid.join(", ")).to_string()
        ))
    })
}

#[derive(Deserialize)]
struct PathQuery {
    #[serde(default)]
    path: String,
    #[serde(default)]
    recursive: bool,
}

/// A file the editor stops at — over the read refusal, or a save over the
/// editable size — is a 413 carrying the size and the limit, so the client
/// can offer to reveal it or say what to cut.
fn too_large(path: &str, size: u64, limit: u64, locale: &bisa_i18n::Locale) -> Response {
    let text = bisa_core::text!(
        "error-node-ide-file-too-large",
        path = path.to_string(),
        size = size.to_string(),
        limit = limit.to_string()
    );
    (
        StatusCode::PAYLOAD_TOO_LARGE,
        Json(json!({
            "error": bisa_i18n::render(locale, &text),
            "text": text,
            "size": size,
            "limit": limit,
        })),
    )
        .into_response()
}

/// A conflict is a 409 that hands back what is actually there.
fn conflict_or(e: EngineError, locale: &bisa_i18n::Locale) -> Response {
    match e {
        EngineError::FileConflict {
            path,
            current_hash,
            current_text,
        } => (
            StatusCode::CONFLICT,
            Json(json!({
                "error": bisa_i18n::render(locale, &bisa_core::text!("error-node-ide-changed-since-read", path = path.clone())),
                "text": bisa_core::text!("error-node-ide-changed-since-read", path = path.clone()),
                "current_hash": current_hash,
                "current_text": current_text,
            })),
        )
            .into_response(),
        other => ApiError::from(other).into_response(),
    }
}

async fn read(
    State(state): State<Shared>,
    AxPath((scope, id)): AxPath<(String, String)>,
    Query(q): Query<PathQuery>,
    crate::i18n::Lang(locale): crate::i18n::Lang,
) -> Result<Response, ApiError> {
    let scope = parse_scope(&scope)?;
    let caps = files::EditorCaps::of(state.engine.inner());
    let ws = Arc::clone(&state.engine.inner().ws);
    let (path, root_id) = (q.path.clone(), id.clone());
    let content = blocking(move || {
        ws.read_file_capped(scope, &root_id, &path, ReadCap(caps.refuse))
            .map_err(EngineError::from)
    })
    .await?;
    if content.size > caps.refuse {
        return Ok(too_large(&q.path, content.size, caps.refuse, &locale));
    }
    let dto = IdeFileDto {
        scope: scope.as_str().to_string(),
        id,
        path: content.path,
        size: content.size,
        binary: content.binary,
        truncated: content.truncated,
        editable: !content.binary && content.size <= caps.editable,
        text: content.text,
        hash: content.hash,
    };
    Ok(Json(dto).into_response())
}

/// `GET /ide/raw/{scope}/{id}?path=` — a file's bytes for a renderer (ide/03
/// §Rendered documents): a PDF, an image, a recording, a sheet, a document.
///
/// Typed by the crate's one rule for bytes into a webview with no CSP
/// (`attachments.rs`): an image is recognised from its header and served
/// inline as that; everything else is `application/octet-stream`, `nosniff`
/// and a download — never a page, whatever the name says. The desktop fetches
/// it with the bearer header and hands the bytes to a renderer itself.
/// Above `RAW_REFUSE_BYTES` the answer is 413 with the size, like the editor's.
async fn raw(
    State(state): State<Shared>,
    crate::i18n::Lang(locale): crate::i18n::Lang,
    AxPath((scope, id)): AxPath<(String, String)>,
    Query(q): Query<PathQuery>,
) -> Result<Response, ApiError> {
    let scope = parse_scope(&scope)?;
    let file = files::raw_file(state.engine.inner(), scope, &id, &q.path)?;
    if file.size > files::RAW_REFUSE_BYTES {
        let text = bisa_core::text!(
            "error-node-ide-file-too-large-to-render",
            path = q.path.clone(),
            size = file.size as i64,
            max = files::RAW_REFUSE_BYTES as i64
        );
        return Ok((
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(json!({
                "error": bisa_i18n::render(&locale, &text),
                "text": text,
                "size": file.size,
                "limit": files::RAW_REFUSE_BYTES,
            })),
        )
            .into_response());
    }
    // The bytes are streamed, never held whole: three renders of a large
    // recording are three open files, not three copies of it in memory. The
    // type is read from the head, then the file starts again from the top.
    use tokio::io::{AsyncReadExt, AsyncSeekExt};
    let mut opened = tokio::fs::File::open(&file.path).await.map_err(|e| {
        crate::not_found(bisa_core::text!(
            "error-node-ide-refused",
            a0 = (q.path).to_string(),
            e = e.to_string()
        ))
    })?;
    let mut head = vec![0u8; 512];
    let n = opened.read(&mut head).await.map_err(|e| {
        crate::not_found(bisa_core::text!(
            "error-node-ide-refused",
            a0 = (q.path).to_string(),
            e = e.to_string()
        ))
    })?;
    head.truncate(n);
    opened
        .seek(std::io::SeekFrom::Start(0))
        .await
        .map_err(|e| ApiError::internal(&e))?;
    let content_type = crate::attachments::image_type(&head).unwrap_or("application/octet-stream");
    let disposition = if content_type == "application/octet-stream" {
        "attachment"
    } else {
        "inline"
    };
    let stream = futures::stream::try_unfold(opened, |mut f| async move {
        let mut buf = vec![0u8; 64 * 1024];
        let n = f.read(&mut buf).await?;
        if n == 0 {
            return Ok::<_, std::io::Error>(None);
        }
        buf.truncate(n);
        Ok(Some((axum::body::Bytes::from(buf), f)))
    });
    Ok((
        [
            (header::CONTENT_TYPE, content_type),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (header::CONTENT_DISPOSITION, disposition),
            (header::CONTENT_LENGTH, &file.size.to_string()),
        ],
        axum::body::Body::from_stream(stream),
    )
        .into_response())
}

async fn write(
    State(state): State<Shared>,
    AxPath((scope, id)): AxPath<(String, String)>,
    Query(q): Query<PathQuery>,
    crate::i18n::Lang(locale): crate::i18n::Lang,
    crate::Body(body): crate::Body<IdeWriteBody>,
) -> Result<Response, ApiError> {
    let scope = parse_scope(&scope)?;
    let caps = files::EditorCaps::of(state.engine.inner());
    if body.text.len() as u64 > caps.editable {
        return Ok(too_large(
            &q.path,
            body.text.len() as u64,
            caps.editable,
            &locale,
        ));
    }
    match files::write_file(
        state.engine.inner(),
        scope,
        &id,
        &q.path,
        &body.text,
        body.base_hash.as_deref(),
    ) {
        Ok(w) => {
            Ok(Json(json!({"path": w.path, "hash": w.hash, "created": w.created})).into_response())
        }
        Err(e) => Ok(conflict_or(e, &locale)),
    }
}

async fn create(
    State(state): State<Shared>,
    AxPath((scope, id)): AxPath<(String, String)>,
    crate::Body(body): crate::Body<IdeCreateBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let scope = parse_scope(&scope)?;
    let kind = match body.kind.as_str() {
        "file" => EntryKind::File,
        "dir" => EntryKind::Dir,
        other => {
            return Err(bad_request(bisa_core::text!(
                "error-node-ide-kind-file-dir",
                other = format!("{other:?}")
            )))
        }
    };
    let inner = Arc::clone(state.engine.inner());
    let at = body.path;
    let path = blocking(move || files::create_entry(&inner, scope, &id, &at, kind)).await?;
    Ok(Json(json!({"path": path, "kind": body.kind})))
}

async fn move_entry(
    State(state): State<Shared>,
    AxPath((scope, id)): AxPath<(String, String)>,
    crate::Body(body): crate::Body<IdeMoveBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let scope = parse_scope(&scope)?;
    let inner = Arc::clone(state.engine.inner());
    let (from, to) = (body.from.clone(), body.to);
    let path = blocking(move || files::move_entry(&inner, scope, &id, &from, &to)).await?;
    Ok(Json(json!({"from": body.from, "path": path})))
}

/// The project a scope's root belongs to, for the settings that apply there —
/// a workstream's project; a goal's, a run's or a work item's folder has none.
fn project_of(state: &Shared, scope: FileScope, id: &str) -> Option<bisa_core::ProjectId> {
    match scope {
        FileScope::Workstream => id
            .parse()
            .ok()
            .and_then(|wid| state.engine.workspace().get_workstream(wid).ok())
            .map(|w| w.project),
        FileScope::Goal | FileScope::Run | FileScope::WorkItem => None,
    }
}

/// `editor.delete.trash`, resolved for this root: how a delete here goes.
fn disposal_for(state: &Shared, scope: FileScope, id: &str) -> Disposal {
    let project = project_of(state, scope, id);
    Disposal::from_setting(files::setting_bool(
        state.engine.inner(),
        project,
        "editor.delete.trash",
        true,
    ))
}

async fn delete(
    State(state): State<Shared>,
    AxPath((scope, id)): AxPath<(String, String)>,
    Query(q): Query<PathQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let scope = parse_scope(&scope)?;
    let disposal = disposal_for(&state, scope, &id);
    let inner = Arc::clone(state.engine.inner());
    let gone =
        blocking(move || files::delete_entry(&inner, scope, &id, &q.path, q.recursive, disposal))
            .await?;
    Ok(Json(
        json!({"ok": true, "path": gone.path, "disposal": gone.disposal}),
    ))
}

/// Several entries as one act (`files::delete_entries`): a refused batch is
/// the 400 a single delete gives, with nothing gone; one the engine took
/// answers 200 with every asked path that went and, when it halted, the
/// first that did not with the reason in the caller's language.
async fn delete_many(
    State(state): State<Shared>,
    AxPath((scope, id)): AxPath<(String, String)>,
    crate::i18n::Lang(locale): crate::i18n::Lang,
    crate::Body(body): crate::Body<IdeDeleteBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let scope = parse_scope(&scope)?;
    let disposal = disposal_for(&state, scope, &id);
    let inner = Arc::clone(state.engine.inner());
    let asked: Vec<files::ToDelete> = body
        .entries
        .into_iter()
        .map(|e| files::ToDelete {
            path: e.path,
            recursive: e.recursive,
        })
        .collect();
    let deletions =
        blocking(move || files::delete_entries(&inner, scope, &id, &asked, disposal)).await?;
    let failed = deletions.halted.map(|halted| {
        json!({
            "path": halted.path,
            "reason": bisa_i18n::render(&locale, &halted.error.text()),
        })
    });
    Ok(Json(json!({
        "ok": failed.is_none(),
        "deleted": deletions.deleted,
        "failed": failed,
        "disposal": disposal,
    })))
}

/// What a delete under this root would do — so the confirmation can say
/// *moved to the Trash* or *removed* before the click, not after.
async fn disposal(
    State(state): State<Shared>,
    AxPath((scope, id)): AxPath<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let scope = parse_scope(&scope)?;
    Ok(Json(json!({"disposal": disposal_for(&state, scope, &id)})))
}

async fn copy_entry(
    State(state): State<Shared>,
    AxPath((scope, id)): AxPath<(String, String)>,
    crate::Body(body): crate::Body<IdeCopyBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let scope = parse_scope(&scope)?;
    let inner = Arc::clone(state.engine.inner());
    let (from, to) = (body.from.clone(), body.to);
    let path = blocking(move || files::copy_entry(&inner, scope, &id, &from, &to)).await?;
    Ok(Json(json!({"from": body.from, "path": path})))
}

async fn watch_root(
    State(state): State<Shared>,
    AxPath((scope, id)): AxPath<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let scope = parse_scope(&scope)?;
    let inner = Arc::clone(state.engine.inner());
    let root = blocking(move || watch::watch(&inner, scope, &id)).await?;
    Ok(Json(json!({
        "watching": true,
        "root": root.display().to_string(),
        "idle_secs": watch::WATCH_IDLE.as_secs(),
    })))
}

async fn unwatch_root(
    State(state): State<Shared>,
    AxPath((scope, id)): AxPath<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let scope = parse_scope(&scope)?;
    let was = watch::unwatch(state.engine.inner(), scope, &id);
    Ok(Json(json!({"watching": false, "was_watching": was})))
}

/// The routes this module mounts, for `bisa-node --bin api-docs` and the
/// route test. Kept beside `routes()` so a route added here is added here.
pub const ROUTES: &[RouteDoc] = &[
    RouteDoc {
        method: "GET",
        path: "/ide/file/{scope}/{id}",
        summary: "A file for the editor (`?path=`): text, `hash`, `editable`; read-only above 2 MiB, 413 above 20 MiB.",
    },
    RouteDoc {
        method: "PUT",
        path: "/ide/file/{scope}/{id}",
        summary: "Save `{text, base_hash?}` at `?path=`; no hash creates; 409 carries `current_hash` and `current_text`.",
    },
    RouteDoc {
        method: "GET",
        path: "/ide/raw/{scope}/{id}",
        summary: "A file's bytes for a renderer (`?path=`): an image typed from its header and served inline, anything else `application/octet-stream` + `nosniff` as a download; 413 with the size above 256 MiB.",
    },
    RouteDoc {
        method: "POST",
        path: "/ide/files/{scope}/{id}",
        summary: "Create `{path, kind: file|dir}` under the writable root; an existing path is refused.",
    },
    RouteDoc {
        method: "DELETE",
        path: "/ide/files/{scope}/{id}",
        summary: "Remove `?path=` — to the OS trash when `editor.delete.trash` is on for the root's project (the default), else unlinked; the answer's `disposal` says which. A directory needs `?recursive=true`; the root itself is never removed.",
    },
    RouteDoc {
        method: "POST",
        path: "/ide/files/{scope}/{id}/delete",
        summary: "Remove several entries as one act — `{entries: [{path, recursive?}]}` — by the same disposal: every entry is checked before anything goes (the root, a path that leaves it, one that is not there, a folder without its `recursive`, or nothing named refuses the whole batch, 400); then the list goes to the OS trash as one move, or is unlinked one after another. The answer's `deleted` lists every asked path that went, `failed: {path, reason}` the first that did not (`null` when all went), `disposal` how.",
    },
    RouteDoc {
        method: "GET",
        path: "/ide/files/{scope}/{id}/disposal",
        summary: "How a delete under this root goes right now: `{disposal: trash|unlink}`, so a confirmation can say so before the click.",
    },
    RouteDoc {
        method: "POST",
        path: "/ide/files/{scope}/{id}/move",
        summary: "Rename or move `{from, to}` within the root; an existing target is refused.",
    },
    RouteDoc {
        method: "POST",
        path: "/ide/files/{scope}/{id}/copy",
        summary: "Duplicate `{from, to}` within the root — a file, or a folder entry by entry, symlinks recreated and never followed; an existing target is refused.",
    },
    RouteDoc {
        method: "POST",
        path: "/ide/watch/{scope}/{id}",
        summary: "Watch the scope's root; `file_changed` frames follow on the engine stream — the working tree, and under `.git/` only `HEAD`, `index`, `packed-refs`, `FETCH_HEAD`, the operation markers, `refs/`, `rebase-merge/`, `rebase-apply/` and `sequencer/`. Re-post to keep the five-minute lease.",
    },
    RouteDoc {
        method: "DELETE",
        path: "/ide/watch/{scope}/{id}",
        summary: "Stop watching the scope's root.",
    },
    RouteDoc {
        method: "GET",
        path: "/ide/search/{scope}/{id}",
        summary: "Content search (`?q=&regex=&case=&word=&include=&exclude=&limit=`), streamed as SSE: `hit` rows, then `done`.",
    },
    RouteDoc {
        method: "GET",
        path: "/ide/index/{scope}/{id}",
        summary: "Every file path under the root, relative and sorted, `.gitignore` honoured, bounded (`?limit=`, 100,000 default) — what quick open scores against.",
    },
    RouteDoc {
        method: "GET",
        path: "/ide/graph/{scope}/{id}",
        summary: "A window of the commit graph (`?from=&count=&refresh=&refs=all|head`): rows with lanes and edges, `total`, `done` once the whole log is laid out, `stale` while a relayout runs. `refs=head` walks only what HEAD reaches; the two are laid out and cached apart.",
    },
    RouteDoc {
        method: "GET",
        path: "/ide/graph/{scope}/{id}/search",
        summary: "Search the whole laid-out log (`?q=&from=&limit=&refs=all|head`, 500 default, 5,000 cap): the matching row `indices` by subject, author, id prefix or ref name, `searched`, `done` once the tail was there to search, `truncated` when the cap stopped it.",
    },
    RouteDoc {
        method: "GET",
        path: "/ide/layout/{scope}/{id}",
        summary: "The workbench layout saved for this root (`{layout}`, `null` when none) — local window furniture, never synced.",
    },
    RouteDoc {
        method: "PUT",
        path: "/ide/layout/{scope}/{id}",
        summary: "Replace the saved layout for this root with the JSON body (capped at 256 KiB).",
    },
    RouteDoc {
        method: "POST",
        path: "/ide/replace/{scope}/{id}",
        summary: "Replace across files: `apply: false` previews every change with each file's `base_hash`; `apply: true` writes each file by compare-and-swap and reports the ones that moved on.",
    },
];
