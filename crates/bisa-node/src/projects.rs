//! Projects and workstreams over HTTP: the control plane for the folders an
//! goal owns and the checkouts work happens in.
//!
//! Four rules shape every route here.
//!
//! **A project is listed two ways, and they answer different questions.**
//! `GET /goals/{id}/projects` is "what can *this* goal see" — the union
//! `own ∪ linked ∪ ancestors'`, each row labelled with how. `GET /projects` is
//! "what is there" — every project in the workspace, once each, listed from
//! its owner's side with `seen_by` naming the goals that can see it. Both go
//! through the same labelling ([`visible_to`]) over the same union rule, so
//! there is one implementation of visibility and two views onto it. Without
//! the second route a workspace-wide screen costs one request per goal plus
//! a fold on the client, which is where the desktop's projects screen was.
//!
//! **Provenance is explicit.** A project arrives one of five ways — `new`,
//! `clone`, `import`, `adopt`, `link` — and the caller says which
//! ([`NewProjectBody`]). A **managed** root — `new`, `clone`, `import` — is a
//! folder this node made, so `new` initialises it as a repository and there is
//! no flag for it; the rule that stayed absolute is the one about folders we
//! did *not* make, so `adopt` writes nothing into the folder it is given, ever.
//!
//! `import` and `adopt` are the two answers to *"here is a folder"*, and the
//! difference is where the files end up living. An import copies the tree —
//! `.git` included, because a repository without its history is not the
//! repository — into a `Managed` root, and the copy is the goal's from then
//! on. An adopt records the folder where it lies and never writes into it.
//! Both **read** the folder they are given and neither modifies it, so the
//! choice is about ownership rather than about risk.
//!
//! **A slug and an adopted path are path-traversal boundaries.** The slug
//! becomes a directory name under the workspace root, so it goes through
//! [`bisa_core::project::validate_slug`]'s allowlist before anything
//! touches the filesystem. An adopted path must be absolute, is canonicalized
//! (which resolves `..` and every symlink, so what is stored is the real
//! directory rather than the route somebody took to it), must be an existing
//! directory, and must not land inside the Bisa workspace — a project
//! rooted at workspace truth could have its workstreams overwrite the journal.
//! An imported path goes through the identical check
//! ([`resolve_source_path`]), because it is the identical question about the
//! identical kind of path.
//!
//! **Push and PR return the gate; they do not wait for it.**
//! [`bisa_engine::projects::push_workstream`] opens a `GateKind::Publish`
//! gate and blocks on a human. An HTTP request that blocks until a person
//! approves is a request that times out, so these routes hand back the gate
//! instead:
//!
//! | project `publish` | response |
//! |---|---|
//! | `Gated` | **`202 Accepted`** with `{"gate": "<id>", "status": "awaiting_publish_gate"}`. Nothing has left the machine. The work continues in the daemon and completes when the gate is decided — from the inbox, `POST /goals/{id}/decide`, or `bisa approve`. A declined gate pushes nothing. |
//! | `Auto` (default) | `200 OK` with the finished result — no gate is opened. |
//! | `Manual` | `409 Conflict`: this project's pushes are a person's job and no gate can widen that. |
//!
//! The route decides which of the three happened by watching, not by
//! re-implementing the policy: it starts the engine call and returns whichever
//! comes first — the gate appearing in the inbox, or the call finishing.
//!
//! **Deleting a record never deletes somebody's work by accident.** `DELETE
//! /projects/{pid}` forgets the record and its links and leaves every file on
//! disk; `?tree=true` additionally takes the whole *managed* project folder
//! off disk the way a file delete goes — the OS Trash (recoverable) unless
//! `editor.delete.trash` is off at the machine — and an
//! adopted external folder is refused outright. `DELETE /workstreams/{wid}`
//! closes the workstream and only removes the checkout on `?tree=true`.
//!
//! This module reads git directly (`bisa_vcs::git`) for status and
//! diffs, which the engine does not expose. Every git **write** still goes
//! through `bisa_engine::projects`, so there is exactly one place that
//! creates, commits, pushes or tears down a working tree.

use crate::dto::*;
use crate::route_docs::RouteDoc;
use crate::tags::{parse_tags, TagFilter};
use crate::Query;
use crate::{bad_request, not_found, parse_id, ApiError, Shared};
use axum::extract::{Path as AxPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use bisa_core::tags::TagEntity;
use bisa_core::Localize as _;
use bisa_core::{
    AgentId, Gate, GoalId, Project, ProjectId, ProjectRoot, PublishPolicy, Slug, Vcs, Workstream,
    WorkstreamId, WorkstreamKind,
};
use bisa_engine::projects as eng;
use bisa_store::{NewProject, WorkstreamFilter};
use bisa_vcs::git;
use serde::Deserialize;
use serde_json::json;
use std::path::{Path as FsPath, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub(crate) fn routes() -> Router<Shared> {
    Router::new()
        .route("/projects", get(list_all).post(create_standalone))
        .route("/projects/{pid}/archive", post(archive))
        .route("/projects/{pid}/git/init", post(init_repository))
        .route("/goals/{id}/projects", get(list).post(create_for_goal))
        .route(
            "/projects/{pid}",
            get(detail).patch(patch_project).delete(remove),
        )
        .route("/projects/{pid}/attach", post(attach).delete(detach))
        .route("/projects/{pid}/assignees", put(set_assignees))
        .route(
            "/projects/{pid}/workstreams",
            get(project_workstreams).post(open_workstream),
        )
        .route(
            "/projects/{pid}/workstream-scripts",
            get(workstream_scripts),
        )
        .route(
            "/projects/{pid}/workstream-scripts/approve",
            post(approve_workstream_scripts),
        )
        .route("/workstreams", get(list_all_workstreams))
        .route(
            "/workstreams/{wid}",
            get(workstream_detail).patch(patch_workstream).delete(close),
        )
        .route("/workstreams/{wid}/diff", get(workstream_diff))
        .route("/workstreams/{wid}/commit", post(commit))
        .route("/workstreams/{wid}/run-command", get(run_command))
        .route("/workstreams/{wid}/status", get(workstream_status))
        .route("/workstreams/{wid}/board/place", put(place_workstream_card))
        .route("/workstreams/status", get(all_workstream_statuses))
        .route(
            "/projects/{pid}/workstreams/status",
            get(project_workstream_statuses),
        )
        .route("/workstreams/{wid}/push", post(push))
        .route("/workstreams/{wid}/push-with-lease", post(push_with_lease))
        .route("/workstreams/{wid}/pr", post(open_pr))
        .route("/workstreams/{wid}/pr/suggest", post(pr_suggest))
        // The git surface of one checkout: the primary for the
        // project's own tree, a worktree for a branch of its own — one code
        // path, keyed by the workstream.
        .route("/workstreams/{wid}/git/status", get(git_status))
        .route("/workstreams/{wid}/git/files", get(git_files))
        .route("/workstreams/{wid}/git/diff", get(git_diff))
        .route("/workstreams/{wid}/git/sides", get(git_sides))
        .route("/workstreams/{wid}/git/stage", post(git_stage))
        .route("/workstreams/{wid}/git/unstage", post(git_unstage))
        .route("/workstreams/{wid}/git/commit", post(git_commit))
        .route("/workstreams/{wid}/git/message", post(git_message))
        .route("/workstreams/{wid}/git/hunk", post(git_hunk))
        .route("/workstreams/{wid}/git/blame", get(git_blame))
        .route("/workstreams/{wid}/git/history", get(git_history))
        .route("/workstreams/{wid}/git/commit/{sha}", get(git_show_commit))
        .route(
            "/workstreams/{wid}/git/commit/{sha}/diff",
            get(git_commit_file_diff),
        )
        .route(
            "/workstreams/{wid}/git/commit/{sha}/sides",
            get(git_commit_file_sides),
        )
        .route("/workstreams/{wid}/git/identity", get(git_identity))
        .route(
            "/workstreams/{wid}/git/connection",
            get(git_connection).post(git_connection_check),
        )
        .route(
            "/workstreams/{wid}/git/connection/check",
            post(git_connection_check),
        )
        .route("/workstreams/{wid}/git/account", put(git_set_account))
        .route(
            "/workstreams/{wid}/git/config",
            get(git_local_config).put(git_set_local_config),
        )
}

// ---------------------------------------------------------------------------
// Ids and containment
// ---------------------------------------------------------------------------

pub(crate) fn parse_project_id(s: &str) -> Result<ProjectId, ApiError> {
    ProjectId::from_str(s).map_err(|_| {
        bad_request(bisa_core::text!(
            "error-node-projects-not-project-id",
            s = format!("{s:?}")
        ))
    })
}

pub(crate) fn parse_workstream_id(s: &str) -> Result<WorkstreamId, ApiError> {
    WorkstreamId::from_str(s).map_err(|_| {
        bad_request(bisa_core::text!(
            "error-node-projects-not-workstream-id",
            s = format!("{s:?}")
        ))
    })
}

/// The slug is a directory name joined onto the workspace root, so it is
/// checked here — before a record is written or a folder created — rather
/// than only in the store. `..`, absolute paths, separators and every
/// non-ASCII lookalike are outside the allowlist by construction.
fn parse_slug(slug: &str) -> Result<Slug, ApiError> {
    Slug::new(slug).map_err(|e| bad_request(e.text()))
}

/// Resolve a caller-supplied folder to the real directory it names, or refuse
/// with the reason.
///
/// Absolute, canonicalized (so `..` and symlinks are resolved rather than
/// stored), an existing directory, and outside the Bisa workspace. The
/// last rule is the one that matters most: a project rooted at
/// `<data-dir>/goals/...` would put a working tree on top of workspace
/// truth, and a symlink is exactly how someone would try to get there.
///
/// `verb` only names the operation in the refusal. `adopt` and `import` ask
/// the identical question of the identical path — one folder the user pointed
/// at, outside the workspace — and giving import its own copy of these four
/// rules would be one rule with a hole in it, in the one place where the hole
/// is a working tree on top of the journal.
///
/// Public because the CLI adopts and imports folders too.
/// `verb` is a key the messages select on — `adopt` · `import` · `install-pet` ·
/// `import-addon` · `validate-addon` · `inspect` — never words; every caller's verb
/// has its own branch in the four `error-node-projects-source-*` messages.
pub fn resolve_source_path(
    workspace_root: &FsPath,
    raw: &str,
    verb: &str,
) -> Result<String, bisa_core::Text> {
    let path = PathBuf::from(raw);
    if !path.is_absolute() {
        return Err(bisa_core::text!(
            "error-node-projects-source-needs-absolute-path",
            verb = verb,
            raw = format!("{raw:?}")
        ));
    }
    let real = path.canonicalize().map_err(|e| {
        bisa_core::text!(
            "error-node-projects-source-cannot",
            verb = verb,
            raw = format!("{raw:?}"),
            e = e.to_string()
        )
    })?;
    if !real.is_dir() {
        return Err(bisa_core::text!(
            "error-node-projects-source-not-directory",
            verb = verb,
            raw = format!("{raw:?}")
        ));
    }
    let root = workspace_root
        .canonicalize()
        .unwrap_or_else(|_| workspace_root.to_path_buf());
    if real.starts_with(&root) {
        return Err(bisa_core::text!(
            "error-node-projects-source-inside-workspace",
            verb = verb,
            path = real.display().to_string()
        ));
    }
    Ok(real.to_string_lossy().into_owned())
}

/// [`resolve_source_path`] for an adopted folder. Kept as its own name because
/// it is what the CLI and the desktop's error mapping already speak.
pub fn resolve_adopt_path(workspace_root: &FsPath, raw: &str) -> Result<String, bisa_core::Text> {
    resolve_source_path(workspace_root, raw, "adopt")
}

fn source_path(state: &Shared, raw: &str, verb: &str) -> Result<String, ApiError> {
    resolve_source_path(state.engine.workspace().root(), raw, verb).map_err(bad_request)
}

fn publish_or_default(p: Option<PublishPolicy>) -> PublishPolicy {
    p.unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Projects
// ---------------------------------------------------------------------------

fn project_row(state: &Shared, project: Project) -> ProjectRow {
    let ws = state.engine.workspace();
    let path = ws.project_root_path(&project);
    let workstreams = ws
        .list_workstreams(WorkstreamFilter::Project(project.id))
        .map(|v| v.len())
        .unwrap_or(0);
    let goals = ws
        .goals_of_project(project.id)
        .unwrap_or_default()
        .iter()
        .map(|g| g.to_string())
        .collect();
    ProjectRow {
        path: path.display().to_string(),
        exists: path.is_dir(),
        workstreams,
        goals,
        project,
    }
}

/// Every project attached to this goal — the attachments, and nothing else.
/// A parent's project is not a child's until somebody attaches
/// it, and a project attached to no goal is a standalone project.
async fn list(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
    filter: TagFilter,
) -> Result<Json<serde_json::Value>, ApiError> {
    let goal = parse_id(&id)?;
    let ws = state.engine.workspace();
    // The attachments, and nothing else: a parent's project is not a child's
    // until somebody attaches it.
    let attached = filter.apply(ws, TagEntity::Project, ws.projects_for(goal)?, |p| {
        p.id.to_string()
    })?;
    let rows: Vec<ProjectRow> = attached
        .into_iter()
        .map(|p| project_row(&state, p))
        .collect();
    Ok(Json(json!({"goal": goal.to_string(), "projects": rows})))
}

/// Every project in the workspace, **once each**, with the goals that can
/// see it.
///
/// `GET /goals/{id}/projects` answers "what can *this* goal see". A
/// workspace-wide project screen and a project picker ask the other question
/// — "what is there" — and the only way to reach it through the per-goal
/// route is one request per goal plus a fold on the client, which is what
/// the desktop used to do.
///
/// Rows are listed from the owner's side, so a project linked into four
/// goals is one row (`visibility: "own"`) with four entries in `seen_by`,
/// not four rows. Order is [`bisa_store::Workspace::list_projects`]'s —
/// oldest first, and stable.
#[derive(Deserialize, Default)]
struct AllQuery {
    /// Include the projects put away — hidden unless asked.
    #[serde(default)]
    archived: bool,
}

async fn list_all(
    State(state): State<Shared>,
    filter: TagFilter,
    Query(q): Query<AllQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let ws = state.engine.workspace();
    let mut all = ws.list_projects()?;
    if q.archived {
        all.extend(ws.list_archived_projects()?);
    }
    let projects = filter.apply(ws, TagEntity::Project, all, |p| p.id.to_string())?;
    let rows: Vec<ProjectRow> = projects
        .into_iter()
        .map(|p| project_row(&state, p))
        .collect();
    Ok(Json(json!({"projects": rows})))
}

/// Create a project — see [`NewProjectBody`] for the four ways in.
/// `POST /goals/{id}/projects`: create, then attach to the goal in the path.
async fn create_for_goal(
    State(state): State<Shared>,
    AxPath(id): AxPath<String>,
    crate::Body(body): crate::Body<NewProjectBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let goal = parse_id(&id)?;
    state.engine.workspace().get_goal(goal)?; // attaching to a ghost is refused first
    create(&state, Some(goal), body).await
}

/// `POST /projects`: a standalone project, attached to nothing. The
/// first-run path — a workspace opens on Projects, and a goal comes later.
async fn create_standalone(
    State(state): State<Shared>,
    crate::Body(body): crate::Body<NewProjectBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if matches!(body, NewProjectBody::Attach { .. }) {
        return Err(bad_request(bisa_core::text!(
            "error-node-projects-attaching-needs-goal-post-goals-projects-post"
        )));
    }
    create(&state, None, body).await
}

async fn create(
    state: &Shared,
    goal: Option<GoalId>,
    body: NewProjectBody,
) -> Result<Json<serde_json::Value>, ApiError> {
    let ws = state.engine.workspace();

    // Where the project is being born: this route's goal, or the workspace.
    // Derived here, never sent by a caller (I28a); only an agent step's
    // intake op can produce a `step` origin.
    let origin = match goal {
        Some(goal) => bisa_core::ProjectOrigin::from_goal(goal),
        None => bisa_core::ProjectOrigin::Workspace,
    };

    let request = match body {
        NewProjectBody::Attach { project } => {
            let pid = parse_project_id(&project)?;
            let Some(goal) = goal else {
                return Err(bad_request(bisa_core::text!(
                    "error-node-projects-attaching-needs-goal"
                )));
            };
            state.engine.attach_project(goal, pid)?;
            let row = project_row(state, ws.get_project(pid)?);
            return Ok(Json(json!({"project": row.project, "path": row.path,
                                  "exists": row.exists, "goals": row.goals,
                                  "attached_to": goal.to_string()})));
        }
        NewProjectBody::New {
            slug,
            name,
            assignees,
            publish,
            tags,
            git_config,
        } => bisa_engine::NewProjectRequest {
            new: NewProject {
                origin: origin.clone(),
                slug: parse_slug(&slug)?,
                name,
                root: ProjectRoot::Managed,
                vcs: Vcs::None,
                assignees: crate::goals::parse_assignees(&assignees)?,
                publish: publish_or_default(publish),
                tags: parse_tags(&tags)?,
            },
            source: bisa_engine::ProjectSource::New,
            git_config: git_config.into_iter().collect(),
        },
        NewProjectBody::Clone {
            slug,
            name,
            url,
            depth,
            assignees,
            publish,
            tags,
            git_config,
        } => {
            if url.trim().is_empty() {
                return Err(bad_request(bisa_core::text!(
                    "error-node-projects-clone-needs-url"
                )));
            }
            bisa_engine::NewProjectRequest {
                new: NewProject {
                    origin: origin.clone(),
                    slug: parse_slug(&slug)?,
                    name,
                    root: ProjectRoot::Managed,
                    vcs: Vcs::None,
                    assignees: crate::goals::parse_assignees(&assignees)?,
                    publish: publish_or_default(publish),
                    tags: parse_tags(&tags)?,
                },
                source: bisa_engine::ProjectSource::Clone { url, depth },
                git_config: git_config.into_iter().collect(),
            }
        }
        NewProjectBody::Adopt {
            slug,
            name,
            path,
            assignees,
            publish,
            tags,
            git_config,
        } => {
            let real = source_path(state, &path, "adopt")?;
            bisa_engine::NewProjectRequest {
                new: NewProject {
                    origin: origin.clone(),
                    slug: parse_slug(&slug)?,
                    name,
                    root: ProjectRoot::External { path: real },
                    // Read by the engine at creation, never created.
                    vcs: Vcs::None,
                    assignees: crate::goals::parse_assignees(&assignees)?,
                    publish: publish_or_default(publish),
                    tags: parse_tags(&tags)?,
                },
                source: bisa_engine::ProjectSource::Adopt,
                git_config: git_config.into_iter().collect(),
            }
        }
        NewProjectBody::Import {
            slug,
            name,
            path,
            assignees,
            publish,
            tags,
            git_config,
        } => {
            // The same four rules an adopted path passes. An import reads this
            // folder and writes only inside the workspace, so a source inside
            // the workspace is the one shape that could make the copy chase
            // its own tail.
            let real = source_path(state, &path, "import")?;
            bisa_engine::NewProjectRequest {
                new: NewProject {
                    origin: origin.clone(),
                    slug: parse_slug(&slug)?,
                    name,
                    // Managed, not External: the point of importing is that
                    // the files become the workspace's.
                    root: ProjectRoot::Managed,
                    vcs: Vcs::None,
                    assignees: crate::goals::parse_assignees(&assignees)?,
                    publish: publish_or_default(publish),
                    tags: parse_tags(&tags)?,
                },
                source: bisa_engine::ProjectSource::Import {
                    source: PathBuf::from(real),
                },
                git_config: git_config.into_iter().collect(),
            }
        }
    };

    // One engine operation makes the record, its primary workstream and the
    // tree, and announces it — or takes the record back. Then the
    // attachment, when a goal asked: the project exists either way, and a
    // failed attach names the project that is waiting.
    let created = eng::create(state.engine.inner(), request).await?;
    let project = created.project;
    if let Some(goal) = goal {
        state.engine.attach_project(goal, project.id)?;
    }

    let row = project_row(state, project);
    Ok(Json(
        json!({"project": row.project, "path": row.path, "exists": row.exists,
               "goals": row.goals, "attached_to": goal.map(|g| g.to_string()),
               "imported": created.imported}),
    ))
}

async fn detail(
    State(state): State<Shared>,
    AxPath(pid): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pid = parse_project_id(&pid)?;
    let ws = state.engine.workspace();
    let project = ws.get_project(pid)?;
    let path = ws.project_root_path(&project);
    let workstreams = ws.list_workstreams(WorkstreamFilter::Project(pid))?;
    let goals: Vec<String> = ws
        .goals_of_project(pid)?
        .iter()
        .map(|g| g.to_string())
        .collect();
    Ok(Json(json!({
        "project": project,
        "path": path.display().to_string(),
        "exists": path.is_dir(),
        "goals": goals,
        "workstreams": workstreams,
    })))
}

async fn patch_project(
    State(state): State<Shared>,
    AxPath(pid): AxPath<String>,
    crate::Body(body): crate::Body<PatchProjectBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pid = parse_project_id(&pid)?;
    let ws = state.engine.workspace();
    let mut project = ws.get_project(pid)?;
    if let Some(name) = body.name {
        if name.trim().is_empty() {
            return Err(bad_request(bisa_core::text!(
                "error-node-projects-project-name-must-not-be-blank"
            )));
        }
        project.name = name;
    }
    if let Some(publish) = body.publish {
        project.publish = publish;
    }
    if let Some(assignees) = &body.assignees {
        project.assignees = crate::goals::parse_assignees(assignees)?;
    }
    if let Some(tags) = &body.tags {
        project.tags = parse_tags(tags)?;
    }
    if let Some(group) = body.group {
        project.group = group
            .map(|g| g.trim().to_string())
            .filter(|g| !g.is_empty());
    }
    if let Some(photo) = body.photo {
        // A photo is a picture this machine holds, and a small one: what
        // every row draws must be cheap to fetch, decode and sync (ide/14
        // §Photos) — the one check every photo passes.
        if let Some(photo) = &photo {
            crate::attachments::photo_check(
                state.engine.workspace(),
                photo,
                bisa_core::PhotoProfile::Picture,
            )?;
        }
        project.photo = photo;
    }
    let project = eng::update(state.engine.inner(), project)?;
    Ok(Json(json!({"project": project})))
}

/// Edit a workstream's person-editable fields: name, note, pinned.
/// Kind, project and state are not edits and are refused by the store.
async fn patch_workstream(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
    crate::Body(body): crate::Body<PatchWorkstreamBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let w = eng::edit_workstream(
        state.engine.inner(),
        wid,
        bisa_engine::WorkstreamEdit {
            name: body.name,
            note: body.note,
            pinned: body.pinned,
            due: body.due,
        },
    )?;
    Ok(Json(json!({"workstream": w})))
}

/// Put a card at an index in a Board column — a view over the workstream,
/// never its lifecycle (ide/16). Answers every record the move rewrote.
async fn place_workstream_card(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
    crate::Body(body): crate::Body<PlaceWorkstreamBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let written =
        eng::place_workstream_card(state.engine.inner(), wid, body.column, body.index as usize)?;
    Ok(Json(json!({"workstreams": written})))
}

#[derive(Deserialize)]
struct TreeQuery {
    /// Remove the working tree as well as the record. Off by default: an
    /// uncommitted branch is the only copy of somebody's work.
    #[serde(default)]
    tree: bool,
}

/// Put a project away or take it back out: `{archived}`. Archiving stops
/// every session in it and hides it from the rail; nothing on disk moves.
async fn archive(
    State(state): State<Shared>,
    AxPath(pid): AxPath<String>,
    crate::Body(body): crate::Body<ArchiveBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pid = parse_project_id(&pid)?;
    let project = state.engine.archive_project(pid, body.archived)?;
    Ok(Json(json!({"project": project_row(&state, project)})))
}

/// Turn a plain-folder project's tree into a git repository, on the person's
/// explicit ask — an adopted folder too, the one write adopt allows:
/// `{git_config?}`. 409 when the record already says git, 400 when the folder
/// is not on disk, 404 unknown.
async fn init_repository(
    State(state): State<Shared>,
    AxPath(pid): AxPath<String>,
    crate::Body(body): crate::Body<InitRepositoryBody>,
) -> Result<Json<InitRepositoryReply>, ApiError> {
    let pid = parse_project_id(&pid)?;
    let done = state.engine.init_repository(pid, body.git_config).await?;
    Ok(Json(InitRepositoryReply {
        project: project_row(&state, done.project),
        committer: done.committer.sentence(),
    }))
}

/// Forget a project. **The folder stays on disk unless `?tree=true`.**
///
/// `tree=true` takes the whole *managed* project folder off disk after its
/// workstreams are torn down — to the OS Trash (recoverable), or unlinked when
/// `editor.delete.trash` is off at the machine; an adopted folder is
/// refused (400). Every session in the project is stopped first. The one
/// implementation is the engine's (`projects::delete`), shared with a
/// retirement and the CLI.
async fn remove(
    State(state): State<Shared>,
    AxPath(pid): AxPath<String>,
    Query(q): Query<TreeQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pid = parse_project_id(&pid)?;
    let deleted = state.engine.delete_project(pid, q.tree).await?;
    Ok(Json(json!({
        "ok": true,
        "removed_tree": deleted.removed_tree,
        "path": deleted.path,
        "workstreams_forgotten": deleted.workstreams_forgotten,
        "kept": deleted.kept,
    })))
}

/// Attach a project to a goal. Idempotent; moves no bytes.
async fn attach(
    State(state): State<Shared>,
    AxPath(pid): AxPath<String>,
    crate::Body(body): crate::Body<ProjectAttachBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pid = parse_project_id(&pid)?;
    let goal = parse_id(&body.goal)?;
    let attachment = state.engine.attach_project(goal, pid)?;
    let row = project_row(&state, state.engine.workspace().get_project(pid)?);
    Ok(Json(json!({
        "project": row.project,
        "goals": row.goals,
        "attached_to": goal.to_string(),
        "attachment": attachment,
    })))
}

/// Detach a project from a goal. The folder, its workstreams and its history
/// stay exactly where they are (invariant I14).
async fn detach(
    State(state): State<Shared>,
    AxPath(pid): AxPath<String>,
    crate::Body(body): crate::Body<ProjectAttachBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pid = parse_project_id(&pid)?;
    let goal = parse_id(&body.goal)?;
    state.engine.detach_project(goal, pid)?;
    let row = project_row(&state, state.engine.workspace().get_project(pid)?);
    Ok(Json(json!({"ok": true, "project": pid.to_string(),
                   "detached_from": goal.to_string(), "goals": row.goals})))
}

/// Who carries this project: its agents take work items that run here, its
/// humans may decide its gates. `{"assignees": []}` clears it.
async fn set_assignees(
    State(state): State<Shared>,
    AxPath(pid): AxPath<String>,
    crate::Body(body): crate::Body<AssigneesBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pid = parse_project_id(&pid)?;
    let ws = state.engine.workspace();
    let mut project = ws.get_project(pid)?;
    project.assignees = crate::goals::parse_assignees(&body.assignees)?;
    let project = eng::update(state.engine.inner(), project)?;
    Ok(Json(json!({"project": project})))
}

// ---------------------------------------------------------------------------
// Git state
// ---------------------------------------------------------------------------

/// Ask git about a working tree. Neutral, never an error, when there is no
/// repository there: a plain folder is a legitimate kind of project.
async fn git_status_info(path: PathBuf, recorded_git: bool) -> GitStatusInfo {
    let exists = path.is_dir();
    if !exists {
        return GitStatusInfo {
            exists: false,
            clean: true,
            ..Default::default()
        };
    }
    tokio::task::spawn_blocking(move || {
        // The disk is the authority on what git can do — a project recorded
        // as git whose folder is not (yet) a repository reports `git: false`
        // rather than an error.
        if !git::is_repo(&path) {
            return GitStatusInfo {
                exists: true,
                clean: true,
                error: recorded_git.then(|| {
                    bisa_i18n::english(&bisa_core::text!(
                        "error-node-projects-recorded-git-not-repository"
                    ))
                }),
                ..Default::default()
            };
        }
        let status = match git::status(&path) {
            Ok(s) => s,
            Err(e) => {
                return GitStatusInfo {
                    git: true,
                    exists: true,
                    error: Some(e.to_string()),
                    ..Default::default()
                }
            }
        };
        GitStatusInfo {
            git: true,
            exists: true,
            branch: status.branch,
            detached: status.detached,
            head: status.oid,
            upstream: status.upstream,
            remote: git::remote_get(&path, "origin").ok().flatten(),
            ahead: status.ahead,
            behind: status.behind,
            staged: status.staged,
            unstaged: status.unstaged,
            untracked: status.untracked,
            conflicted: status.conflicted,
            clean: status.is_clean,
            in_progress: git::in_progress(&path).ok().flatten().map(Into::into),
            error: None,
        }
    })
    .await
    .unwrap_or_default()
}

async fn changed_files(path: PathBuf) -> Vec<ChangedFile> {
    tokio::task::spawn_blocking(move || {
        git::diff_stat(&path)
            .unwrap_or_default()
            .into_iter()
            .map(|c| ChangedFile {
                path: c.path.display().to_string(),
                old_path: c.old_path.map(|p| p.display().to_string()),
                kind: serde_json::to_value(c.kind)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_string))
                    .unwrap_or_else(|| "unknown".into()),
                insertions: c.insertions,
                deletions: c.deletions,
                binary: c.binary,
            })
            .collect()
    })
    .await
    .unwrap_or_default()
}

/// Branch, ahead/behind, remote and dirty counts for a workstream's checkout —
/// the project's own tree when the workstream is the primary.
async fn git_status(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let ws = state.engine.workspace();
    let w = ws.get_workstream(wid)?;
    let project = ws.get_project(w.project)?;
    let path = ws.checkout_in(&project, &w);
    let status = git_status_info(path.clone(), recorded_git(&w, &project)).await;
    let default_branch = match &project.vcs {
        Vcs::Git { default_branch, .. } => Some(default_branch.clone()),
        Vcs::None => None,
    };
    Ok(Json(json!({
        "workstream": wid.to_string(),
        "project": project.id.to_string(),
        "path": path.display().to_string(),
        "publish": project.publish,
        "default_branch": default_branch,
        "status": status,
    })))
}

// ---------------------------------------------------------------------------
// A project's own working tree, file by file
// ---------------------------------------------------------------------------

fn file_row(f: bisa_vcs::FileStatus) -> GitFileRow {
    GitFileRow {
        path: f.path.display().to_string(),
        old_path: f.old_path.as_ref().map(|p| p.display().to_string()),
        index: f.index.to_string(),
        worktree: f.worktree.to_string(),
        staged: f.is_staged(),
        unstaged: f.is_unstaged(),
        untracked: f.untracked,
        conflicted: f.is_conflicted(),
        conflict: f.conflict.map(Into::into),
    }
}

pub(crate) fn rows(files: Vec<bisa_vcs::FileStatus>) -> Vec<GitFileRow> {
    files.into_iter().map(file_row).collect()
}

/// The changed files of a checkout as the wire carries them — for the
/// command line, which answers in the wire's shape whether the node or an
/// engine of its own did the work.
pub fn file_rows(files: Vec<bisa_vcs::FileStatus>) -> Vec<GitFileRow> {
    rows(files)
}

/// Every path git has something to say about, with both of its letters.
///
/// A read, and only a read: it lists what is there and stages nothing, for
/// the same reason `workstream_diff` refuses to — a GET that changed the index
/// would mean opening a screen was an edit.
async fn git_files(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let ws = state.engine.workspace();
    let w = ws.get_workstream(wid)?;
    let path = ws.workstream_checkout(&w)?;
    if !path.is_dir() {
        return Err(not_found(bisa_core::text!(
            "error-node-projects-workstream-has-no-checkout",
            wid = wid.to_string(),
            a0 = (path.display()).to_string()
        )));
    }
    let files = tokio::task::spawn_blocking(move || git::status_files(&path))
        .await
        .map_err(|e| ApiError::internal(&e))?
        .map_err(|e| {
            ApiError::text(
                StatusCode::BAD_REQUEST,
                bisa_core::text!("error-node-projects-refused", detail = e.to_string()),
            )
        })?;
    Ok(Json(json!({
        "workstream": wid.to_string(),
        "clean": files.is_empty(),
        "files": rows(files),
    })))
}

#[derive(Deserialize)]
struct FileDiffQuery {
    /// Repository-root-relative, as `git/files` reported it.
    path: String,
    /// The index against HEAD instead of the working tree against the index.
    #[serde(default)]
    staged: bool,
}

/// One file's patch.
///
/// An **untracked** file comes back with an empty `diff` and `untracked:
/// true` rather than a patch: `git diff` cannot show a file git has never
/// seen without staging it first, and a GET does not stage. The same rule
/// `workstream_diff` follows, at file scale.
async fn git_diff(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
    Query(q): Query<FileDiffQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let ws = state.engine.workspace();
    let root = ws.workstream_checkout(&ws.get_workstream(wid)?)?;
    let (spec, staged) = (q.path.clone(), q.staged);
    let (diff, files) = tokio::task::spawn_blocking(move || {
        let diff = git::diff_file(&root, &spec, staged)?;
        let files = git::status_files(&root)?;
        Ok::<_, bisa_vcs::VcsError>((diff, files))
    })
    .await
    .map_err(|e| ApiError::internal(&e))?
    .map_err(|e| {
        ApiError::text(
            StatusCode::BAD_REQUEST,
            bisa_core::text!("error-node-projects-refused", detail = e.to_string()),
        )
    })?;
    let row = files.into_iter().find(|f| f.path == FsPath::new(&q.path));
    // Cut at the same cap a commit's diff is, on a line, and said: a
    // generated lock file's patch is not a 400 MB answer.
    let (diff, truncated) = bisa_engine::ide::git::cap_diff(diff);
    Ok(Json(json!({
        "workstream": wid.to_string(),
        "path": q.path,
        "staged": q.staged,
        "diff": diff,
        "truncated": truncated,
        "untracked": row.as_ref().is_some_and(|f| f.untracked),
        "file": row.map(file_row),
    })))
}

/// The two whole texts of one file's change, for a comparison drawn side
/// by side or inline (ide/04 §The Changes view). A read, never a stage:
/// an untracked file's left side is `null`, as is a deleted file's right.
async fn git_sides(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
    Query(q): Query<FileDiffQuery>,
) -> Result<Json<FileSides>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let sides =
        bisa_engine::ide::git::file_sides(state.engine.inner(), wid, q.path.clone(), q.staged)
            .await?;
    Ok(Json(FileSides {
        workstream: wid.to_string(),
        path: q.path,
        staged: q.staged,
        binary: sides.original.binary || sides.modified.binary,
        truncated: sides.original.truncated || sides.modified.truncated,
        original: sides.original.text,
        modified: sides.modified.text,
    }))
}

/// Put paths in the index. Index-only, so there is nothing here to undo but
/// the index — see `unstage`.
async fn git_stage(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
    crate::Body(body): crate::Body<StageBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let files = eng::stage_in(state.engine.inner(), wid, body.paths).await?;
    Ok(Json(
        json!({"workstream": wid.to_string(), "files": rows(files)}),
    ))
}

/// Take paths back out of the index. **The files themselves are not
/// touched** — this is `git restore --staged`, and the crate that runs it
/// cannot spell the version without the flag.
async fn git_unstage(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
    crate::Body(body): crate::Body<StageBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let files = eng::unstage_in(state.engine.inner(), wid, body.paths).await?;
    Ok(Json(
        json!({"workstream": wid.to_string(), "files": rows(files)}),
    ))
}

/// The workstream's conversation (ide/09): the analogue of a goal
/// thread, where agents are reachable inside the checkout they work in. The
/// primary's thread is the project's.
/// Stage one hunk — or the lines of one — by patching the index (ide/04 §6).
/// `reverse` unstages it. The working tree is untouched either way.
async fn git_hunk(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
    crate::Body(body): crate::Body<HunkBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let files =
        bisa_engine::ide::git::stage_hunk(state.engine.inner(), wid, body.patch, body.reverse)
            .await?;
    Ok(Json(
        json!({"workstream": wid.to_string(), "files": rows(files)}),
    ))
}

#[derive(Deserialize)]
struct BlameQuery {
    path: String,
    #[serde(default)]
    start: Option<u32>,
    #[serde(default)]
    end: Option<u32>,
}

/// Who last touched each line of a file (`git blame --porcelain`).
async fn git_blame(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
    Query(q): Query<BlameQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let range = match (q.start, q.end) {
        (Some(s), Some(e)) => Some((s, e)),
        (Some(s), None) => Some((s, s)),
        (None, Some(_)) => {
            return Err(bad_request(bisa_core::text!(
                "error-node-projects-end-without-start"
            )))
        }
        (None, None) => None,
    };
    let mut lines =
        bisa_engine::ide::git::blame(state.engine.inner(), wid, q.path.clone(), range).await?;
    // A blame is one row per line; a generated file's tens of thousands are
    // cut at the cap and the answer says so.
    let truncated = lines.len() > BLAME_CAP;
    lines.truncate(BLAME_CAP);
    Ok(Json(json!({
        "workstream": wid.to_string(),
        "path": q.path,
        "lines": lines,
        "truncated": truncated,
    })))
}

#[derive(Deserialize)]
struct HistoryQuery {
    path: String,
    #[serde(default)]
    limit: Option<usize>,
}

/// The commits that touched a path, newest first, across renames.
async fn git_history(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
    Query(q): Query<HistoryQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let commits = bisa_engine::ide::git::history(
        state.engine.inner(),
        wid,
        q.path.clone(),
        q.limit.unwrap_or(100),
    )
    .await?;
    Ok(Json(json!({
        "workstream": wid.to_string(),
        "path": q.path,
        "commits": commits,
    })))
}

/// One commit for the inspector: message, refs, the files it changed against
/// its first parent, and its patch (cut at 2 MiB, `truncated: true`).
async fn git_show_commit(
    State(state): State<Shared>,
    AxPath((wid, sha)): AxPath<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let (detail, diff, truncated) =
        bisa_engine::ide::git::commit(state.engine.inner(), wid, sha).await?;
    Ok(Json(json!({
        "workstream": wid.to_string(),
        "commit": detail,
        "diff": diff,
        "truncated": truncated,
    })))
}

/// `?path=` — one file of a commit, repository-root-relative as the commit's
/// file list names it.
#[derive(Deserialize)]
struct CommitPathQuery {
    path: String,
}

/// One file's patch in one commit, against the first parent (ide/05) — the
/// *Hunks* view of a commit's file; cut at 2 MiB and said. A path the
/// commit did not touch is a 400.
async fn git_commit_file_diff(
    State(state): State<Shared>,
    AxPath((wid, sha)): AxPath<(String, String)>,
    Query(q): Query<CommitPathQuery>,
) -> Result<Json<CommitFileDiff>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let (diff, truncated) =
        bisa_engine::ide::git::commit_file(state.engine.inner(), wid, sha.clone(), q.path.clone())
            .await?;
    Ok(Json(CommitFileDiff {
        workstream: wid.to_string(),
        sha,
        path: q.path,
        diff,
        truncated,
    }))
}

/// The two whole texts of one file's change in one commit, for a comparison
/// drawn side by side or inline (ide/05): the first parent's version — at
/// the old path for a rename, nothing for a root commit — against the
/// commit's. A read of objects; nothing in the tree moves.
async fn git_commit_file_sides(
    State(state): State<Shared>,
    AxPath((wid, sha)): AxPath<(String, String)>,
    Query(q): Query<CommitPathQuery>,
) -> Result<Json<CommitFileSides>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let sides = bisa_engine::ide::git::commit_file_sides(
        state.engine.inner(),
        wid,
        sha.clone(),
        q.path.clone(),
    )
    .await?;
    Ok(Json(CommitFileSides {
        workstream: wid.to_string(),
        sha,
        path: q.path,
        binary: sides.original.binary || sides.modified.binary,
        truncated: sides.original.truncated || sides.modified.truncated,
        original: sides.original.text,
        modified: sides.modified.text,
    }))
}

/// Commit a workstream's checkout, staging what the caller selected.
///
/// **No `Publish` gate, deliberately.** The gate stands in front of `push`
/// and `pr` because those leave the machine and cannot be recalled; a commit
/// is local, and the user's own git can undo it. Gating it would teach people
/// to click through the gate that matters.
///
/// `409` on a clean tree, or on a selection that matched nothing changed —
/// the same refusal a workstream commit gives, by the same name.
/// Who will author commits in this checkout's repository. Never journaled —
/// a person's git configuration, not project history.
async fn git_identity(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
) -> Result<Json<GitIdentityView>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let identity = bisa_engine::ide::git::identity(state.engine.inner(), wid).await?;
    let profile = bisa_engine::ide::connection::identity_profile(state.engine.inner(), wid).await?;
    let mut view: GitIdentityView = identity.into();
    view.profile = profile;
    // Only a repository nobody commits in is offered an account's identity,
    // and the host is asked with a bound: a slow or absent host is no
    // suggestion, said at debug, never a failed read of who commits.
    if view.source == crate::dto::IdentitySource::None {
        view.suggested = match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            bisa_engine::ide::git::committer_suggestion(state.engine.inner(), wid),
        )
        .await
        {
            Ok(Ok(s)) => s.map(|s| CommitterSuggestionView {
                name: s.ident.name,
                email: s.ident.email,
                login: s.login,
            }),
            Ok(Err(e)) => {
                tracing::debug!(target: "bisa_node", workstream = %wid, "no committer suggestion: {e}");
                None
            }
            Err(_) => {
                tracing::debug!(target: "bisa_node", workstream = %wid, "no committer suggestion: the code host did not answer in time");
                None
            }
        };
    }
    Ok(Json(view))
}

/// The connection facts for a checkout (ide/04 §The Repository view): the
/// remote and its protocol, the profile, who commits, the transport, the
/// account, and the cautions. Local reads only.
async fn git_connection(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
) -> Result<Json<bisa_engine::ide::connection::RepoConnection>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    Ok(Json(
        bisa_engine::ide::connection::connection(state.engine.inner(), wid).await?,
    ))
}

/// The three read-only probes for a checkout: the code host's word on the
/// bound account and its access, the SSH handshake, `git ls-remote`.
async fn git_connection_check(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
) -> Result<Json<bisa_engine::ide::connection::ConnectionCheck>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    Ok(Json(
        bisa_engine::ide::connection::check(state.engine.inner(), wid).await?,
    ))
}

/// Pin the repository to one account, or unpin it.
async fn git_set_account(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
    crate::Body(body): crate::Body<crate::dto::AccountPinBody>,
) -> Result<Json<bisa_engine::ide::connection::RepoConnection>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    Ok(Json(
        bisa_engine::ide::connection::set_account(state.engine.inner(), wid, body.login).await?,
    ))
}

/// The project's git config as the two layers hold it — every schema key,
/// what the repository sets and what it inherits.
async fn git_local_config(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
    crate::i18n::Lang(locale): crate::i18n::Lang,
) -> Result<Json<GitConfigView>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let view = bisa_engine::ide::git::local_config(state.engine.inner(), wid).await?;
    Ok(Json(GitConfigView::localized(view, &locale)))
}

/// Write the repository's **local** git config: `{set, unset}`, schema keys
/// only. The one door for a project's identity — answering the platform's
/// *who commits?* and committing a settlement the missing identity had
/// refused. Answers the fresh view.
async fn git_set_local_config(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
    crate::i18n::Lang(locale): crate::i18n::Lang,
    crate::Body(body): crate::Body<GitConfigWrite>,
) -> Result<Json<GitConfigView>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let (set, unset) = body.into_parts();
    let view =
        bisa_engine::ide::git::set_local_config(state.engine.inner(), wid, set, unset).await?;
    Ok(Json(GitConfigView::localized(view, &locale)))
}

async fn git_commit(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
    crate::Body(body): crate::Body<WorkstreamCommitBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    if body.message.trim().is_empty() {
        return Err(bad_request(bisa_core::text!(
            "error-node-ide-interactive-commit-needs-message"
        )));
    }
    let commit = eng::commit_in(state.engine.inner(), wid, body.message.trim(), body.paths).await?;
    let ws = state.engine.workspace();
    let root = ws.workstream_checkout(&ws.get_workstream(wid)?)?;
    let files = tokio::task::spawn_blocking(move || git::status_files(&root).unwrap_or_default())
        .await
        .unwrap_or_default();
    Ok(Json(json!({
        "workstream": wid.to_string(),
        "commit": commit.as_str(),
        "short": commit.short(),
        "files": rows(files),
    })))
}

/// Ask the core agent for a commit message for what is staged.
///
/// **It suggests; nothing here commits.** The session it runs is given no MCP
/// servers and a read-only tier ceiling, so it could not act on the project
/// if it wanted to, and the message it returns is a draft in the user's box.
/// A commit carries the user's name, so the last word stays theirs.
///
/// Always `200`, even when there is no answer: a failure comes back as
/// `{"suggested": false, "message": "", "error": "<why>"}`, which leaves the
/// user with an empty box and a sentence explaining it. Anything else would
/// tempt a client into showing a message the agent did not write.
async fn git_message(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    // The workstream must exist; whether an agent answers is a separate question.
    state.engine.workspace().get_workstream(wid)?;
    match eng::suggest_commit_message(state.engine.inner(), wid).await {
        Ok(message) => Ok(Json(json!({
            "workstream": wid.to_string(),
            "suggested": true,
            "message": message,
            "agent": AgentId::GENERAL,
            "error": serde_json::Value::Null,
        }))),
        Err(e) => Ok(Json(json!({
            "workstream": wid.to_string(),
            "suggested": false,
            "message": "",
            "agent": AgentId::GENERAL,
            "error": e.to_string(),
        }))),
    }
}

/// Ask the core agent for a pull request's title and description, from the
/// branch's commits and its diff against the base.
///
/// **It suggests; nothing here pushes or opens anything** — the same
/// read-only session as [`git_message`], and the draft lands in the person's
/// dialog, where the last word is theirs.
///
/// Always `200`, even when there is no answer: a failure — no harness, no
/// model, a branch with nothing beyond its base — comes back as
/// `{"suggested": false, "title": "", "body": "", "error": "<why>"}`, so the
/// dialog keeps what it holds and says why.
async fn pr_suggest(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    // The workstream must exist; whether an agent answers is a separate question.
    state.engine.workspace().get_workstream(wid)?;
    match eng::suggest_pull_request(state.engine.inner(), wid).await {
        Ok(draft) => Ok(Json(json!({
            "workstream": wid.to_string(),
            "suggested": true,
            "title": draft.title,
            "body": draft.body,
            "agent": AgentId::GENERAL,
            "error": serde_json::Value::Null,
        }))),
        Err(e) => Ok(Json(json!({
            "workstream": wid.to_string(),
            "suggested": false,
            "title": "",
            "body": "",
            "agent": AgentId::GENERAL,
            "error": e.to_string(),
        }))),
    }
}

// ---------------------------------------------------------------------------
// Workstreams
// ---------------------------------------------------------------------------

fn branch_of(w: &Workstream) -> Option<(&str, &str)> {
    match &w.kind {
        WorkstreamKind::Worktree { branch, base } => Some((branch, base)),
        WorkstreamKind::Primary | WorkstreamKind::Copy => None,
    }
}

/// Whether git has anything to say about this checkout: a worktree always,
/// the primary when its project is a repository, a copy never.
fn recorded_git(w: &Workstream, project: &Project) -> bool {
    match w.kind {
        WorkstreamKind::Worktree { .. } => true,
        WorkstreamKind::Primary => project.vcs.is_git(),
        WorkstreamKind::Copy => false,
    }
}

/// [`Workstream::require_repository`] as a 400: a category error, not a
/// failure.
pub(crate) fn require_repository(w: &Workstream) -> Result<(), ApiError> {
    w.require_repository().map_err(|e| {
        bad_request(bisa_core::text!(
            "error-node-projects-refused",
            detail = e.to_string()
        ))
    })
}

/// [`Workstream::require_own_branch`] as a 400 — what only a branch cut for
/// one piece of work can do: a pull request, a forced push.
pub(crate) fn require_git_workstream(w: &Workstream) -> Result<(), ApiError> {
    w.require_own_branch().map(|_| ()).map_err(|e| {
        bad_request(bisa_core::text!(
            "error-node-projects-refused",
            detail = e.to_string()
        ))
    })
}

async fn project_workstreams(
    State(state): State<Shared>,
    AxPath(pid): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pid = parse_project_id(&pid)?;
    let ws = state.engine.workspace();
    let project = ws.get_project(pid)?; // existence, so an unknown id is a 400 not an empty list
    let workstreams = ws.list_workstreams(WorkstreamFilter::Project(pid))?;
    let checkouts: Vec<serde_json::Value> = workstreams
        .iter()
        .map(|w| {
            let path = ws.checkout_in(&project, w);
            json!({"workstream": w.id.to_string(), "path": path.display().to_string(),
                   "exists": path.is_dir()})
        })
        .collect();
    Ok(Json(json!({
        "project": pid.to_string(),
        "workstreams": workstreams,
        "checkouts": checkouts,
    })))
}

/// Every workstream in the workspace, oldest first.
///
/// The workspace-wide counterpart to `/projects/{pid}/workstreams`, and the same
/// argument as `GET /projects`: a surface that wants one flat list would
/// otherwise ask every project and fold the answers, which is N requests to
/// draw one list. `GET /projects` can only report a workstream *count* per row
/// precisely because this did not exist.
///
/// Each row carries `project_name`, because a branch name on its own does not
/// say which repository it is in and two projects may well both have a
/// `work/fix-total-…`.
async fn list_all_workstreams(
    State(state): State<Shared>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let ws = state.engine.workspace();
    let projects: std::collections::HashMap<ProjectId, Project> =
        ws.list_projects()?.into_iter().map(|p| (p.id, p)).collect();
    let rows: Vec<serde_json::Value> = ws
        .list_workstreams(WorkstreamFilter::All)?
        .into_iter()
        .map(|w| {
            let project = projects.get(&w.project);
            let path = project.map(|p| ws.checkout_in(p, &w));
            json!({
                "project_name": project.map(|p| p.name.clone()),
                "path": path.as_ref().map(|p| p.display().to_string()),
                "exists": path.as_ref().is_some_and(|p| p.is_dir()),
                "workstream": w,
            })
        })
        .collect();
    Ok(Json(json!({ "workstreams": rows })))
}

/// Open a workstream by hand — a branch and a checkout with no work item behind
/// it.
///
/// The engine's `open_workstream` is written for the executor and takes the
/// work item it is opening for; there is no work item here, so this passes a
/// spec that exists only to carry the goal, the branch label and the agent,
/// and then clears `work_item` on the record it produced. A hand-opened
/// workstream therefore has `work_item: null` — which is the shape the engine's
/// journalling already expects — rather than a reference to an item that was
/// never planned.
async fn open_workstream(
    State(state): State<Shared>,
    AxPath(pid): AxPath<String>,
    crate::Body(body): crate::Body<NewWorkstreamBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pid = parse_project_id(&pid)?;
    let ws = state.engine.workspace();
    let project = ws.get_project(pid)?;

    // The goal the workstream is *for*, when there is one. A workstream made from
    // the IDE has none; one made for a goal must name a goal the
    // project is attached to.
    let goal = match &body.goal {
        Some(raw) => {
            let goal = parse_id(raw)?;
            if !ws.is_attached(goal, pid)? {
                return Err(bad_request(bisa_core::text!(
                    "error-node-projects-goal-not-attached-project-attach-first",
                    goal = goal.to_string(),
                    a0 = (project.slug).to_string()
                )));
            }
            Some(goal)
        }
        None => None,
    };

    let label = body
        .label
        .clone()
        .unwrap_or_else(|| project.slug.to_string())
        .trim()
        .to_string();
    let request = bisa_engine::WorkstreamRequest {
        goal,
        work_item: None,
        label,
        agent: body.agent.clone(),
        source: body.source.clone(),
        base: body.base.clone().filter(|b| !b.trim().is_empty()),
    };
    let place = eng::open_workstream_for(state.engine.inner(), &project, request).await?;
    if place.workstream.is_primary() {
        // An unborn HEAD has nothing to branch from; the engine says so by
        // handing back the primary — the project root — rather than a branch.
        return Err(ApiError::text(
            StatusCode::CONFLICT,
            bisa_core::text!(
                "error-node-projects-project-has-no-commits-yet-make-first",
                a0 = (project.slug).to_string()
            ),
        ));
    }
    Ok(Json(
        json!({"workstream": place.workstream, "path": place.cwd.display().to_string()}),
    ))
}

/// The project's workstream scripts (ide/07 §Workstream scripts) as Git ›
/// Repository shows them: each phase's text and whether **this machine** will
/// run it. The texts are settings, written like any other; the trust is the
/// engine's to compute, so the desktop never hashes.
fn scripts_view(
    inner: &bisa_engine::Inner,
    project: &bisa_core::Project,
) -> Result<crate::dto::WorkstreamScriptsView, ApiError> {
    let scripts = bisa_engine::scripts::status(inner, project)?;
    let timeout_secs = inner
        .ws
        .setting(bisa_engine::scripts::TIMEOUT_KEY, Some(project.id))?
        .value
        .as_u64()
        .unwrap_or(300);
    Ok(crate::dto::WorkstreamScriptsView {
        project: project.id.to_string(),
        timeout_secs,
        scripts,
    })
}

async fn workstream_scripts(
    State(state): State<Shared>,
    AxPath(pid): AxPath<String>,
) -> Result<Json<crate::dto::WorkstreamScriptsView>, ApiError> {
    let pid = parse_project_id(&pid)?;
    let project = state.engine.workspace().get_project(pid)?;
    Ok(Json(scripts_view(state.engine.inner(), &project)?))
}

/// Approve the project's current scripts on this machine — the one write of
/// `workstreams.script.trusted`. Answers the view, now trusted.
async fn approve_workstream_scripts(
    State(state): State<Shared>,
    AxPath(pid): AxPath<String>,
) -> Result<Json<crate::dto::WorkstreamScriptsView>, ApiError> {
    let pid = parse_project_id(&pid)?;
    let project = state.engine.workspace().get_project(pid)?;
    bisa_engine::scripts::approve(state.engine.inner(), &project)?;
    Ok(Json(scripts_view(state.engine.inner(), &project)?))
}

/// The project's run command for this checkout (ide/18): what the IDE's
/// Terminal menu opens in a terminal — the text, whether this machine
/// approved it, and the checkout it runs in. 404 when the project sets none.
async fn run_command(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let (project, checkout) = eng::checkout_tree_pub(state.engine.inner(), wid)?;
    let run =
        bisa_engine::scripts::run_command(state.engine.inner(), &project)?.ok_or_else(|| {
            not_found(bisa_core::text!(
                "error-node-projects-project-sets-no-run-command-set-one"
            ))
        })?;
    Ok(Json(json!({
        "workstream": wid.to_string(),
        "command": run.command,
        "trusted": run.trusted,
        "digest": run.digest,
        "cwd": checkout.display().to_string(),
    })))
}

async fn workstream_detail(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let ws = state.engine.workspace();
    let w = ws.get_workstream(wid)?;
    let project = ws.get_project(w.project)?;
    let path = ws.checkout_in(&project, &w);
    let status = git_status_info(path.clone(), recorded_git(&w, &project)).await;
    let (branch, base) = match branch_of(&w) {
        Some((b, base)) => (Some(b.to_string()), Some(base.to_string())),
        None => (None, None),
    };
    // A fresh branch has no upstream, so `status.ahead` is zero and useless.
    // What a reviewer wants is the distance from where the branch started.
    let ahead_of_base = match (&base, status.git) {
        (Some(base), true) => {
            let (path, base) = (path.clone(), base.clone());
            tokio::task::spawn_blocking(move || git::ahead_behind(&path, &base).ok())
                .await
                .ok()
                .flatten()
        }
        _ => None,
    };
    let changes = if status.git {
        changed_files(path.clone()).await
    } else {
        vec![]
    };
    Ok(Json(json!({
        "workstream": w,
        "path": path.display().to_string(),
        "project": project,
        "branch": branch,
        "base": base,
        "ahead_of_base": ahead_of_base.map(|(a, _)| a),
        "behind_base": ahead_of_base.map(|(_, b)| b),
        "status": status,
        "changes": changes,
    })))
}

/// What is not committed yet: the unified diff against HEAD, the per-file
/// summary, and the untracked files.
///
/// `diff` and `changes` cover **tracked** changes, because that is what `git
/// diff` can show without staging anything — and staging on a read is not
/// something a GET may do. A brand-new file is therefore reported in
/// `untracked` rather than in the patch, and `clean` counts it: a workstream
/// holding only new files is emphatically not clean, and saying otherwise
/// would be the misleading answer.
async fn workstream_diff(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let ws = state.engine.workspace();
    let w = ws.get_workstream(wid)?;
    require_repository(&w)?;
    let path = ws.workstream_checkout(&w)?;
    if !path.is_dir() {
        return Err(not_found(bisa_core::text!(
            "error-node-projects-workstream-has-no-checkout",
            wid = wid.to_string(),
            a0 = (path.display()).to_string()
        )));
    }
    let root = path.clone();
    let (diff, untracked) = tokio::task::spawn_blocking(move || {
        let diff = git::diff_head(&root)?;
        let untracked: Vec<String> = git::list_untracked(&root)
            .unwrap_or_default()
            .into_iter()
            .map(|p| p.display().to_string())
            .collect();
        Ok::<_, bisa_vcs::VcsError>((diff, untracked))
    })
    .await
    .map_err(|e| ApiError::internal(&e))?
    .map_err(|e| {
        ApiError::text(
            StatusCode::BAD_REQUEST,
            bisa_core::text!("error-node-projects-refused", detail = e.to_string()),
        )
    })?;
    let changes = changed_files(path).await;
    Ok(Json(json!({
        "workstream": wid.to_string(),
        "clean": diff.trim().is_empty() && untracked.is_empty(),
        "diff": diff,
        "changes": changes,
        "untracked": untracked,
    })))
}

async fn commit(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
    crate::Body(body): crate::Body<CommitBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    if body.message.trim().is_empty() {
        return Err(bad_request(bisa_core::text!(
            "error-node-ide-interactive-commit-needs-message"
        )));
    }
    let ws = state.engine.workspace();
    require_repository(&ws.get_workstream(wid)?)?;
    let commit = eng::commit_workstream(state.engine.inner(), wid, body.message.trim()).await?;
    Ok(Json(json!({
        "workstream": ws.get_workstream(wid)?,
        "commit": commit.as_str(),
        "short": commit.short(),
    })))
}

async fn close(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
    Query(q): Query<TreeQuery>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    if state.engine.workspace().get_workstream(wid)?.is_primary() {
        return Err(ApiError::text(
            StatusCode::CONFLICT,
            bisa_store::primary_is_the_project(),
        ));
    }
    // Removing the checkout is the one destructive half: it is consented and
    // a recovery ref is written first (ide/07). Closing the record alone is
    // not. Either way the engine stops what stands in the checkout first and
    // says how many; the desktop closes the tabs.
    let (closed, recovery) = if q.tree {
        let consent = crate::ide::consent::from_request(&headers, &state)?;
        let removed = bisa_engine::ide::interactive::close_workstream_removing_tree(
            state.engine.inner(),
            wid,
            consent,
        )
        .await?;
        (removed.closed, removed.recovery)
    } else {
        (
            eng::close_workstream(state.engine.inner(), wid, false).await?,
            None,
        )
    };
    state.engine.inner().ide_status.invalidate(wid);
    Ok(Json(json!({
        "workstream": closed.workstream,
        "removed_tree": q.tree,
        "recovery": recovery,
        "stopped_sessions": closed.stopped_sessions,
    })))
}

/// One workstream's live status (ide/07), cached for two seconds.
async fn workstream_status(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let s = bisa_engine::ide::git::workstream_status(state.engine.inner(), wid).await?;
    Ok(Json(json!({"status": s})))
}

/// The live status of every open workstream of a project — the switcher's rows.
async fn project_workstream_statuses(
    State(state): State<Shared>,
    AxPath(pid): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pid = parse_project_id(&pid)?;
    state.engine.workspace().get_project(pid)?;
    let s = bisa_engine::ide::git::workstream_statuses(state.engine.inner(), Some(pid)).await?;
    Ok(Json(json!({"project": pid.to_string(), "statuses": s})))
}

/// The most lines a blame answers with.
const BLAME_CAP: usize = 20_000;

/// The live status of every open workstream in the workspace.
async fn all_workstream_statuses(
    State(state): State<Shared>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let s = bisa_engine::ide::git::workstream_statuses(state.engine.inner(), None).await?;
    Ok(Json(json!({"statuses": s})))
}

// ---------------------------------------------------------------------------
// The Publish gate over HTTP
// ---------------------------------------------------------------------------

/// A forced push for a rewritten workstream branch — `--force-with-lease`
/// only, never the project's default branch, through the same Publish gate
/// as an ordinary push, and consented: the request's token mints the
/// `HumanConsent` the tier requires.
async fn push_with_lease(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
    headers: axum::http::HeaderMap,
) -> Result<Response, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let ws = state.engine.workspace();
    let w = ws.get_workstream(wid)?;
    require_git_workstream(&w)?;
    let consent = crate::ide::consent::from_request(&headers, &state)?;
    let inner = Arc::clone(state.engine.inner());
    match gate_or_result(&state, wid, async move {
        bisa_engine::ide::interactive::push_workstream_with_lease(&inner, wid, consent).await
    })
    .await?
    {
        Published::Gate(gate) => Ok((
            StatusCode::ACCEPTED,
            Json(json!({
                "workstream": wid.to_string(),
                "gate": gate,
                "status": "awaiting_publish_gate",
                "pushed": false,
            })),
        )
            .into_response()),
        Published::Done(recovery) => Ok(Json(json!({
            "workstream": wid.to_string(),
            "pushed": true,
            "recovery": recovery,
        }))
        .into_response()),
    }
}

/// What a publishing call did before this request had to answer.
pub(crate) enum Published<T> {
    /// A `Publish` gate is open and the work is waiting on a human.
    Gate(String),
    /// It finished without a gate (`PublishPolicy::Auto`).
    Done(T),
}

/// How long to wait for the gate to appear (or the call to finish) before
/// giving up. Generous for a `git push` to a slow remote under `Auto`, and
/// irrelevant under `Gated`, where the gate opens in milliseconds.
const PUBLISH_WAIT: Duration = Duration::from_secs(120);

/// Start a publishing call and return whichever comes first: the `Publish`
/// gate appearing in the inbox, or the call finishing.
///
/// This is what lets the route hand back a gate id instead of blocking until
/// a human decides. The engine owns the policy — `Auto` never opens a gate,
/// `Manual` fails immediately, `Gated` opens one — and this only watches, so
/// the two can never disagree.
pub(crate) async fn gate_or_result<T, F>(
    state: &Shared,
    workstream: WorkstreamId,
    call: F,
) -> Result<Published<T>, ApiError>
where
    F: std::future::Future<Output = Result<T, bisa_engine::EngineError>> + Send + 'static,
    T: Send + 'static,
{
    let subject = format!("workstream:{workstream}");
    let mut task = tokio::spawn(call);
    let deadline = Instant::now() + PUBLISH_WAIT;
    loop {
        tokio::select! {
            biased;
            joined = &mut task => {
                let value = joined
                    .map_err(|e| ApiError::text(StatusCode::INTERNAL_SERVER_ERROR, bisa_core::text!("error-node-projects-publish-task-did-not-finish", e = e.to_string())))??;
                return Ok(Published::Done(value));
            }
            _ = tokio::time::sleep(Duration::from_millis(20)) => {
                if let Some(gate) = state
                    .engine
                    .inbox()
                    .into_iter()
                    .find(|g| g.gate == Gate::Publish && g.subject == subject)
                {
                    // The task stays alive in the daemon: it is waiting on the
                    // gate and completes when a human decides.
                    return Ok(Published::Gate(gate.id));
                }
                if Instant::now() > deadline {
                    return Err(ApiError::text(StatusCode::GATEWAY_TIMEOUT, bisa_core::text!("error-node-projects-publishing-workstream-neither-finished-nor-opened-gate", workstream = workstream.to_string())));
                }
            }
        }
    }
}

/// Push the branch to `origin` — **through the `Publish` gate**.
///
/// `202` with the gate id when a human has to sign for it; `200` when the
/// project publishes automatically; `409` when it publishes manually. Nothing
/// has left the machine when this returns `202`.
async fn push(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
) -> Result<Response, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let ws = state.engine.workspace();
    let w = ws.get_workstream(wid)?;
    require_repository(&w)?;
    let inner = Arc::clone(state.engine.inner());
    match gate_or_result(&state, wid, async move {
        eng::push_workstream(&inner, wid).await
    })
    .await?
    {
        Published::Gate(gate) => Ok((
            StatusCode::ACCEPTED,
            Json(json!({
                "workstream": wid.to_string(),
                "gate": gate,
                "status": "awaiting_publish_gate",
                "pushed": false,
            })),
        )
            .into_response()),
        Published::Done(()) => Ok(Json(json!({
            "workstream": ws.get_workstream(wid)?,
            "status": "pushed",
            "pushed": true,
        }))
        .into_response()),
    }
}

/// Open a pull request on the code host behind `origin` — **through the `Publish` gate**, with
/// the same three outcomes as [`push`].
async fn open_pr(
    State(state): State<Shared>,
    AxPath(wid): AxPath<String>,
    crate::Body(body): crate::Body<PrBody>,
) -> Result<Response, ApiError> {
    let wid = parse_workstream_id(&wid)?;
    let ws = state.engine.workspace();
    let w = ws.get_workstream(wid)?;
    require_git_workstream(&w)?;
    if body.title.trim().is_empty() {
        return Err(bad_request(bisa_core::text!(
            "error-node-projects-pull-request-needs-title"
        )));
    }
    let inner = Arc::clone(state.engine.inner());
    let req = bisa_engine::codehost::PrRequest {
        title: body.title.trim().to_string(),
        body: body.body.unwrap_or_default(),
        draft: body.draft,
        reviewers: body.reviewers,
        labels: body.labels,
    };
    let call = async move { bisa_engine::codehost::open_pr(&inner, wid, req).await };
    match gate_or_result(&state, wid, call).await? {
        Published::Gate(gate) => Ok((
            StatusCode::ACCEPTED,
            Json(json!({
                "workstream": wid.to_string(),
                "gate": gate,
                "status": "awaiting_publish_gate",
                "opened": false,
            })),
        )
            .into_response()),
        Published::Done(pr) => Ok(Json(json!({
            "workstream": ws.get_workstream(wid)?,
            "status": "pr_open",
            "opened": true,
            "pr": {"number": pr.number, "url": pr.url},
        }))
        .into_response()),
    }
}

/// The routes this module mounts, for `bisa-node --bin api-docs` and the
/// route test. Kept beside `routes()` so a route added here is added here.
pub const ROUTES: &[RouteDoc] = &[
    RouteDoc { method: "GET", path: "/projects", summary: "Every project once, with the goals it is attached to; archived projects are left out unless `?archived=true`." },
    RouteDoc { method: "POST", path: "/projects", summary: "Create a standalone project — `new`, `clone`, `import` or `adopt` — attached to nothing." },
    RouteDoc { method: "GET", path: "/goals/{id}/projects", summary: "The projects attached to this goal." },
    RouteDoc { method: "POST", path: "/goals/{id}/projects", summary: "Create a project and attach it to this goal in one call." },
    RouteDoc { method: "GET", path: "/projects/{pid}", summary: "One project with its path, its goals and its workstreams." },
    RouteDoc { method: "PATCH", path: "/projects/{pid}", summary: "Edit a project: `{name?, publish?, assignees?, tags?, group?, photo?}` — `group` and `photo` clear with `null`. A `photo` must be an attachment this machine holds, a picture by its bytes (PNG, JPEG, GIF, WebP), and within 512 KiB (`MAX_PHOTO_BYTES`) — the desktop scales one to a 256 px square first; 400 by name otherwise." },
    RouteDoc { method: "DELETE", path: "/projects/{pid}", summary: "Forget a project: every session in it stopped, its records gone. The folder stays unless `?tree=true`, which takes the whole managed folder off disk the way a file delete goes — the OS Trash, or unlinked when `editor.delete.trash` is off; an adopted folder is never touched (400)." },
    RouteDoc { method: "POST", path: "/projects/{pid}/archive", summary: "`{archived: true}` puts a project away — every session in it stopped, hidden from the rail, refused for an attachment or a step's placement; nothing on disk moves — `{archived: false}` takes it back out. → `{project}` (a row)." },
    RouteDoc { method: "POST", path: "/projects/{pid}/git/init", summary: "Turn a plain-folder project's tree into a git repository on the person's explicit ask — an adopted folder too, the one write adopt allows: `{git_config?: [[key, value]]}`, the local config a creation request can name. `git init` unless the folder already is one, the who-commits policy as at creation, an empty root commit when HEAD is unborn and an identity resolves; existing copy workstreams stay copies. → `{project, committer}` (a row and the policy's sentence, or `null`); 409 when the record already says git, 400 when the folder is not on disk, 404 unknown." },
    RouteDoc { method: "POST", path: "/projects/{pid}/attach", summary: "Attach the project to a goal: `{goal}`. Idempotent; nothing on disk moves." },
    RouteDoc { method: "DELETE", path: "/projects/{pid}/attach", summary: "Detach the project from a goal: `{goal}`. Workstreams and history stay." },
    RouteDoc { method: "GET", path: "/workstreams/{wid}/git/status", summary: "Branch, ahead/behind, remote and dirty counts of the checkout, the operation git has left in progress (`in_progress`), with the project's publish policy and default branch; neutral for a plain folder." },
    RouteDoc { method: "GET", path: "/workstreams/{wid}/git/files", summary: "Every path git has something to say about, with both status letters." },
    RouteDoc { method: "GET", path: "/workstreams/{wid}/git/diff", summary: "One file's patch (`?path=&staged=`)." },
    RouteDoc { method: "GET", path: "/workstreams/{wid}/git/sides", summary: "The two whole texts of one file's change (`?path=&staged=`), for a comparison: the index and the working tree, or HEAD and the index. A side git does not hold is null; a binary side says so and carries no text; a side over the cap is cut on a line and says `truncated`. A read, never a stage." },
    RouteDoc { method: "POST", path: "/workstreams/{wid}/git/stage", summary: "Stage paths: `{paths}`. Index only." },
    RouteDoc { method: "POST", path: "/workstreams/{wid}/git/unstage", summary: "Unstage paths: `{paths}`. The files themselves are untouched." },
    RouteDoc { method: "POST", path: "/workstreams/{wid}/git/commit", summary: "Commit the checkout, staging what `{paths}` names: `{message, paths?}`; 409 on a clean tree, 400 when nobody is set to commit here." },
    RouteDoc { method: "POST", path: "/workstreams/{wid}/git/message", summary: "Ask the general agent for a commit message for what is staged." },
    RouteDoc { method: "POST", path: "/workstreams/{wid}/git/hunk", summary: "Stage one hunk or a few of its lines: `{patch, reverse}` → `git apply --cached`. Index only." },
    RouteDoc { method: "GET", path: "/workstreams/{wid}/git/blame", summary: "Who last touched each line (`?path=&start=&end=`)." },
    RouteDoc { method: "GET", path: "/workstreams/{wid}/git/history", summary: "The commits that touched a path, newest first, following renames (`?path=&limit=`)." },
    RouteDoc { method: "GET", path: "/workstreams/{wid}/git/commit/{sha}", summary: "One commit for the inspector: message, refs, files against the first parent (renames detected, `old_path`), and its whole patch (cut at 2 MiB, `truncated`)." },
    RouteDoc { method: "GET", path: "/workstreams/{wid}/git/commit/{sha}/diff", summary: "One file's patch in one commit against the first parent (`?path=`), for the Hunks view of a commit's file; cut at 2 MiB, `truncated`. A path the commit did not touch is a 400." },
    RouteDoc { method: "GET", path: "/workstreams/{wid}/git/commit/{sha}/sides", summary: "The two whole texts of one file's change in one commit (`?path=`), for a comparison: the first parent's version — at the old path for a rename, null for a root commit or a new file — against the commit's, null for a deleted file; a binary side says so; a side over the cap is cut and says `truncated`." },
    RouteDoc { method: "GET", path: "/workstreams/{wid}/git/identity", summary: "Who will author commits in this checkout’s repository: `user.name`/`user.email`, their `source` (`local` · `global` · `none`), the `profile` the value came from when a profile by organization supplied it, and the global pair, for pinning; `suggested` — the identity the checkout's connected code host account would commit as, offered when nobody is set, never written by itself." },
    RouteDoc { method: "GET", path: "/workstreams/{wid}/git/connection", summary: "What this checkout will use when it talks to its remote (ide/04 §The Repository view): the `remote` (`url`, `protocol`: `https` · `ssh` · `scp` · `local` · `other`, `host`, the `alias` it was written as, `owner`, `name`), the `code_host`, the `profile` it falls under, the `identity` with its `source` and `profile`, the `transport` (over SSH the `key` ssh would offer and whether ssh-agent has it `loaded`; over HTTPS git’s `helpers` and the `helper_username`), the `account` with its `source` (`local` · `profile` · `global` · `only_stored` · `env` · `none`), and the `cautions` (`identity_differs_from_profile` · `key_not_loaded` · `no_account` · `account_outside_owner` · `credential_username_differs` · `key_shared_across_accounts`), each with a sentence. Local reads and one offline `ssh -G`; nothing reaches the remote." },
    RouteDoc { method: "POST", path: "/workstreams/{wid}/git/connection/check", summary: "Three read-only probes for this checkout: the code host’s word on the bound account (`code_host`, a connection) and its `access` to the repository (`found`, `push`), the SSH handshake for an SSH remote (`ssh`, a greeting), and `git ls-remote --heads origin` (`ls_remote`: `ok`, `heads`, `detail`). Nothing on either side changes." },
    RouteDoc { method: "PUT", path: "/workstreams/{wid}/git/account", summary: "Pin this repository to one code host account — its local `codehost.account` — with `{login}`, or unpin it with `{login: null}`. Answers the fresh connection." },
    RouteDoc { method: "GET", path: "/workstreams/{wid}/git/config", summary: "The project’s git config: the schema and every key’s `local` · `global` · `effective` value." },
    RouteDoc { method: "PUT", path: "/workstreams/{wid}/git/config", summary: "Write the repository’s local git config: `{set: {key: value}, unset: [key]}`, schema keys only; 400 names the first refusal. Answers the fresh view, raises `committer_set` when an identity now resolves, and commits a settlement the missing identity had refused." },
    RouteDoc { method: "PUT", path: "/projects/{pid}/assignees", summary: "Who carries this project: its agents take work here, its humans decide its gates." },
    RouteDoc { method: "GET", path: "/projects/{pid}/workstreams", summary: "The project's workstreams, the primary first, with where each checkout is." },
    RouteDoc { method: "POST", path: "/projects/{pid}/workstreams", summary: "Open a workstream by hand: `{goal?, label?, agent?, source?, base?}` — a branch and a checkout. `source` says where it starts, by its `source` word: `new_branch {name?, start?}` (a typed name is sanitised, never refused, but one that exists is refused by name — open it as a `local_branch`), `local_branch {name}`, `remote_branch {remote, name}` (fetched, then a local branch tracking it), `tag {name, branch?, create_at?}` (a new branch at the tag, `from/<tag>` unless named; the tag is made first when `create_at` names a ref), `pull_request {number}` (an open one on the code host behind `origin`: its head fetched and tracked, its base as `base`, the record born `pr_open`; a closed or merged one is a 409 `pull_request_state`). Absent, a derived new branch at `base` — the project's default branch unless given. The project's pre-create script runs first and its failure is a 409 `script_failed` with `detail.phase` and `detail.output`; the post-create script runs once the checkout exists. A repository with no commit answers 409." },
    RouteDoc { method: "GET", path: "/projects/{pid}/workstream-scripts", summary: "The project's workstream scripts — `pre_create`, `post_create`, `clean`, and `run` (ide/18) — each with its text and whether this machine has approved it (`trusted`), plus `timeout_secs`. The texts are the `workstreams.script.*` settings; write them through `PUT /settings/project`." },
    RouteDoc { method: "GET", path: "/workstreams/{wid}/run-command", summary: "The project's run command for this checkout (ide/18): `{command, trusted, digest, cwd}` — what the IDE's Terminal menu opens in a terminal, once this machine approved it (`POST /projects/{pid}/workstream-scripts/approve`). 404 when the project sets none." },
    RouteDoc { method: "POST", path: "/projects/{pid}/workstream-scripts/approve", summary: "Approve the project's current scripts on this machine: their digests join the machine-scoped `workstreams.script.trusted`. A script that is not approved here never runs here. Answers the same view." },
    RouteDoc { method: "GET", path: "/projects/{pid}/workstreams/status", summary: "Live status of the project's open workstreams — branch, ahead/behind base and upstream, dirty counts, running agents — cached two seconds per workstream." },
    RouteDoc { method: "GET", path: "/workstreams/status", summary: "Live status of every open workstream in the workspace, cached." },
    RouteDoc { method: "GET", path: "/workstreams/{wid}/status", summary: "One workstream's live status, cached two seconds." },
    RouteDoc { method: "GET", path: "/workstreams", summary: "Every workstream in the workspace, each project's primary first, then oldest first." },
    RouteDoc { method: "GET", path: "/workstreams/{wid}", summary: "One workstream with its diff summary." },
    RouteDoc { method: "PATCH", path: "/workstreams/{wid}", summary: "Edit a workstream: `{name?, note?, pinned?, due?}` — `name`, `note` and `due` (a `YYYY-MM-DD` day on the Board) clear with `null`. Kind, project and state are not edits." },
    RouteDoc { method: "PUT", path: "/workstreams/{wid}/board/place", summary: "Put a card at an index in a Board column: `{column, index}` — `backlog` · `todo` · `doing` · `done` · `archived`. A view, never the lifecycle: nothing about the checkout moves. Answers every record the move rewrote; a closed workstream is Archived and nothing else (400)." },
    RouteDoc { method: "DELETE", path: "/workstreams/{wid}", summary: "Close a workstream (409 for the primary — remove the project instead). Every session standing in it is stopped first — the agents the engine runs aborted, the harnesses a person opened in its terminals ended on the roster (the desktop closes their tabs) — answered as `stopped_sessions`; `?tree=true` also removes the checkout — consented, with a recovery ref written first, returned as `recovery`. Answers `{workstream, removed_tree, recovery, stopped_sessions}`." },
    RouteDoc { method: "GET", path: "/workstreams/{wid}/diff", summary: "The uncommitted diff against HEAD, per file and whole." },
    RouteDoc { method: "POST", path: "/workstreams/{wid}/commit", summary: "Commit the workstream: `{message}`; 409 on a clean tree." },
    RouteDoc { method: "POST", path: "/workstreams/{wid}/push", summary: "Push the branch — through the `publish` gate: 200 done, 202 gate open, 409 refused with a `code` (`publish_manual` · `publish_no_goal` · `publish_declined` · `nothing_to_publish` · `workstream_state`). The record is reconciled with the checkout first, so a commit made in a terminal counts." },
    RouteDoc { method: "POST", path: "/workstreams/{wid}/push-with-lease", summary: "Push a rewritten workstream branch with `--force-with-lease` — never the project's default branch, through the `publish` gate, consented; a recovery ref is written first. 409 when the remote moved." },
    RouteDoc { method: "POST", path: "/workstreams/{wid}/pr", summary: "Open a pull request on the code host behind `origin` — through the `publish` gate: `{title, body?, draft?, reviewers?, labels?}`; only what `GET /codehost/capabilities/{pid}` allows is sent. A branch not on the remote yet is pushed first under the same gate; 409 with a `code` as for `push`." },
    RouteDoc { method: "POST", path: "/workstreams/{wid}/pr/suggest", summary: "Ask the general agent for a pull request's title and description from the branch's commits and its diff against the base (`base...HEAD`) — read-only, nothing is pushed or opened. Always 200: `{suggested, title, body, agent, error}`; `suggested: false` with the reason when no agent answers or the branch has nothing beyond its base." },
];
