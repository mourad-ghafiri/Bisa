//! Projects on disk, and the workstream lifecycle.
//!
//! `bisa-store` owns project and workstream **records**; this module owns
//! everything that touches the filesystem or a version control binary. The
//! split is deliberate: a record is cheap, reversible and syncable, while a
//! `git init` is none of those things.
//!
//! Three rules hold everything here together.
//!
//! **Never write into a folder you did not create.** M13 narrowed the older
//! rule ("creating a project makes a directory and stops") to this one, and
//! the narrowing is the whole change: a **managed** root is a folder
//! Bisa just made under the owning goal, so creating one now runs
//! [`init_git`] in it as a matter of course — a project whose work is meant
//! to end in commits should not have to be told twice to be a repository, and
//! `--git` had become a flag people forgot and then wondered why their
//! workstreams were throwaway copies. An **external** root is somebody else's
//! directory: adopt records where it is and writes nothing into it, ever, and
//! that half of the rule did not move an inch. [`import`] is the same rule
//! pointed the other way — it *reads* an outside folder and writes only
//! inside the workspace, so the source of an import ends up as untouched as
//! the source of an adopt, and its `vcs` is still read off the copy rather
//! than manufactured.
//!
//! [`materialize`] itself still only makes the directory. It is on the
//! workstream path as well as the creation path, and a `git init` fired from
//! there would turn every copy-backed run into a repository nobody asked for.
//!
//! **For a git project, the workstream is the isolation.** A `git worktree` on
//! its own branch gives a separate working tree whose result is a commit
//! rather than a throwaway `.patch`, and it persists after the run. A non-git
//! project keeps the `bisa-iso` copy floor — the one placement left whose
//! tree is torn down, and therefore the only one that still settles into a
//! patch. An item that names no project reaches none of this: it runs in its
//! goal's own `work/`, which needs no isolation because it is already the
//! item's own folder.
//!
//! **Outward actions pass the `Publish` gate.** Pushing a branch or opening a
//! pull request leaves the machine and cannot be taken back, so
//! [`push_workstream`] and [`open_pr`] open a `GateKind::Publish` gate and wait
//! on a human — unless the project opted into `PublishPolicy::Auto`. Under
//! `PublishPolicy::Manual` they refuse outright; a person runs the push.
//!
//! Every `git` call goes through [`tokio::task::spawn_blocking`], because
//! `bisa-vcs` is deliberately synchronous.

use crate::events::{EngineEvent, EnginePayload};
use crate::identity::{self, CommitRetry, Committer, CommitterReason};
use crate::scripts;
use crate::{warn_on_err, EngineError, Inner};
use bisa_core::event::JournalPayload;
use bisa_core::workitem::WorkItemSpec;
use bisa_core::WorkflowRun;
use bisa_core::{
    resolve_step_project, AgentId, AskKind, Gate, GoalId, Home, Project, ProjectId, ProjectRoot,
    PublishPolicy, StepId, StepKind, StepProject, Vcs, WorkItemId, Workstream, WorkstreamId,
    WorkstreamKind, WorkstreamSource, WorkstreamState, WorkstreamTransition,
};
use bisa_store::{NewProject, WorkstreamFilter};
use bisa_vcs::{git, CommitId, FileStatus, VcsError};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

// ---------------------------------------------------------------------------
// Plumbing
// ---------------------------------------------------------------------------

/// Run a synchronous VCS call off the async runtime.
pub(crate) async fn blocking<T, F>(f: F) -> Result<T, EngineError>
where
    F: FnOnce() -> Result<T, VcsError> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| {
            EngineError::Invalid(bisa_core::text!(
                "error-engine-invalid-vcs-task-did-not-finish",
                e = e.to_string()
            ))
        })?
        .map_err(EngineError::from)
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Where a work item's workstreams live: under the **goal**, never inside the
/// project. An adopted external repository stays clean, and deleting a goal
/// takes its scratch with it.
///
/// Delegated rather than joined by hand. This spelled the same layout as
/// `GoalPaths::workstreams` independently once, which is precisely the
/// drift `bisa_store::paths` exists to prevent: two correct answers that
/// nothing forces to stay equal, and a file-tree route that reads one of them
/// while the engine writes the other.
/// Where a workstream's checkout lives: under its project, never under a goal.
/// Detaching the project from the goal moves no bytes.
fn workstream_path(inner: &Inner, project: &Project, id: WorkstreamId) -> PathBuf {
    inner.ws.paths().project(&project.slug).workstream_dir(id)
}

// ---------------------------------------------------------------------------
// Branch naming
// ---------------------------------------------------------------------------

/// How many characters of the instructions go into a branch name. Long enough
/// to recognise the work at a glance in `git branch`, short enough that the
/// unique suffix stays visible.
const BRANCH_SLUG_CHARS: usize = 40;

/// The branch a work item's workstream gets: readable **and** unique.
///
/// `work/<first 40 chars of the instructions, slugified>-<last 6 of the item
/// ULID>` — e.g. `work/implement-the-parser-a3f9k2`. The prose half is what a
/// human reads in `git branch`; the ULID tail is what stops two items that
/// begin with the same sentence ("Fix the failing test in …") from colliding.
///
/// Everything is funnelled through [`bisa_core::branch_name_for`], so
/// unicode, slashes, control characters, `..`, `.lock` and an all-punctuation
/// instruction all fold into a legal ref rather than failing a run. When the
/// prose sanitises away to nothing the ULID tail carries the name on its own,
/// which is ugly but still unique — losing uniqueness would be the real bug.
///
/// `full_id` swaps the 6-character tail for the whole ULID. It is the retry
/// used when `git` refuses the short name because that ref already exists.
pub fn branch_for(instructions: &str, item: WorkItemId, full_id: bool) -> String {
    branch_for_ulid(instructions, item.0, full_id)
}

/// The same, keyed on any ULID: a workstream opened from the IDE has no work
/// item, and its own id is the tail that keeps the branch unique.
pub fn branch_for_ulid(instructions: &str, id: ulid::Ulid, full_id: bool) -> String {
    let head: String = instructions.chars().take(BRANCH_SLUG_CHARS).collect();
    let id = id.to_string();
    let suffix = if full_id {
        id.as_str()
    } else {
        // A ULID is 26 ASCII characters, so slicing by bytes is safe.
        &id[id.len().saturating_sub(6)..]
    };
    bisa_core::branch_name_for("work", &format!("{head}-{suffix}"))
}

/// What a workstream is opened for, and from. A work item's carries its goal
/// and item and starts a derived new branch; one made in the IDE carries
/// neither and names its [`WorkstreamSource`] — a new branch, an existing
/// one, a remote's, a tag, a pull request (ide/07 §Where a workstream starts).
#[derive(Debug, Clone)]
pub struct WorkstreamRequest {
    pub goal: Option<GoalId>,
    pub work_item: Option<WorkItemId>,
    /// What a derived branch name is made from: the item's instructions, or
    /// the label a person typed.
    pub label: String,
    pub agent: Option<String>,
    /// Where the checkout starts. A typed new-branch name is sanitised by the
    /// same rule as a derived name, never refused — but a name that already
    /// exists is refused rather than silently checked out: the source for
    /// that is the branch itself.
    pub source: WorkstreamSource,
    /// The branch the work goes back to — what the lifecycle diffs against and
    /// a pull request targets. `None` is the project's default branch; a
    /// pull request source brings its own.
    pub base: Option<String>,
}

impl WorkstreamRequest {
    /// A workstream for a work item: its goal's when it has one — a run of
    /// the workspace's item names no goal, and its run finds the workstream
    /// by the item (`WorkstreamFilter::Run`).
    fn for_item(spec: &WorkItemSpec) -> Self {
        Self {
            goal: spec.home.goal(),
            work_item: Some(spec.id),
            label: spec.instructions.clone(),
            agent: spec.agent.clone(),
            source: WorkstreamSource::default(),
            base: None,
        }
    }
}

/// A branch name a person typed, made safe the way derived names are:
/// `kind/slug` when they typed a slash, `work/<slug>` otherwise. Never
/// refused — a name with a stray character becomes the nearest legal one.
pub fn typed_branch_name(typed: &str) -> String {
    let t = typed.trim().trim_matches('/');
    match t.split_once('/') {
        Some((kind, rest)) if !rest.is_empty() => bisa_core::branch_name_for(kind, rest),
        _ => bisa_core::branch_name_for("work", t),
    }
}

// ---------------------------------------------------------------------------
// Materialization
// ---------------------------------------------------------------------------

/// Ensure the project's folder exists (managed) or is really there
/// (external), and return its path.
///
/// **This never runs `git init`.** Creation paths reach [`init_git`] on their
/// own for a managed root; this one does not, because [`open_workstream`] calls
/// it too and initialising a repository from the workstream path would silently
/// convert a project the user chose to keep plain. An `External` root is
/// verified and left alone — adopting a folder must not mutate it.
pub async fn materialize(inner: &Inner, project: &Project) -> Result<PathBuf, EngineError> {
    let path = inner.ws.project_root_path(project);
    match &project.root {
        ProjectRoot::Managed => {
            std::fs::create_dir_all(&path)?;
        }
        ProjectRoot::External { .. } => {
            if !path.is_dir() {
                return Err(EngineError::Invalid(bisa_core::text!(
                    "error-engine-invalid-project-points-which-not-directory",
                    a0 = (project.slug).to_string(),
                    a1 = (path.display()).to_string()
                )));
            }
        }
    }
    Ok(path)
}

/// Turn a project's folder into a git repository, and record what it became.
///
/// Every path that creates a **managed** project calls this: the folder is
/// one Bisa just made under the owning goal, so there is nobody whose
/// directory is being altered and nothing to ask about. What the old `--git`
/// flag protected against — initialising *somebody's* directory, with the
/// hooks, ignore rules and history that follow — is the `External` case, and
/// the only caller that reaches this with one is [`init_repository`]: a
/// person's explicit ask, because adopt itself writes nothing.
///
/// Idempotent: a folder that is already a repository is adopted rather than
/// re-initialised, which is what makes it safe on a re-run and on a cloned or
/// imported tree that arrived with its own `.git`.
pub async fn init_git(inner: &Inner, project: &Project) -> Result<Project, EngineError> {
    let path = materialize(inner, project).await?;
    let default_branch = blocking({
        let path = path.clone();
        move || {
            if !git::is_repo(&path) {
                git::init(&path)?;
            }
            git::default_branch(&path)
        }
    })
    .await?;
    let remote = blocking({
        let path = path.clone();
        move || git::remote_get(&path, "origin")
    })
    .await?;
    let mut project = project.clone();
    project.vcs = Vcs::Git {
        default_branch,
        remote,
        // Offline, from `origin` alone: which kind, which instance, which repository.
        code_host: crate::codehost::record_for(inner, &path).await,
    };
    Ok(inner.ws.update_project(project)?)
}

/// What a person's *Initialise a repository* answers: the project as it now
/// reads, and what the committer policy did for the new repository.
#[derive(Debug, Clone)]
pub struct Initialised {
    pub project: Project,
    pub committer: Committer,
}

/// Turn an existing plain-folder project's tree into a git repository, on a
/// person's explicit ask — `POST /projects/{pid}/git/init`, `bisa project
/// git-init`, and the one card the IDE draws wherever it says *a plain folder*
/// (ide/04, ide/07, ide/13).
///
/// Unlike [`init_git`], reached for **both** roots — an adopted `External`
/// folder too. Adopt writes nothing because nothing asked it to; this *is* the
/// ask, and the one write it allows. Refused with [`EngineError::Conflict`]
/// when the record already says git (the button is never drawn then, so this
/// is a double click or a stale caller); idempotent against the *disk* as
/// `init_git` is, so a folder that grew a `.git` behind the record's back is
/// recorded, not re-initialised.
///
/// Then the who-commits policy runs exactly as it does at creation
/// ([`identity::apply_on_creation`]), and — only when HEAD is unborn and an
/// identity resolves — an empty root commit gives the first workstream a
/// commit to branch from. History this call did not make is never committed
/// over, and no identity means no commit: the desk holds the ask, and the next
/// workstream runs in the primary until who commits is set, exactly as a new
/// managed project does (see *Where a workstream starts*).
///
/// Workstreams the project already has stay what they are: a copy keeps
/// working as a copy; only the next one opened is a branch.
pub async fn init_repository(
    inner: &Inner,
    pid: ProjectId,
    git_config: Vec<(String, String)>,
) -> Result<Initialised, EngineError> {
    let project = inner.ws.get_project(pid)?;
    if project.vcs.is_git() {
        return Err(EngineError::Conflict(bisa_core::text!(
            "error-engine-conflict-project-already-repository",
            a0 = (project.slug).to_string()
        )));
    }
    let project = init_git(inner, &project).await?;
    let committer = identity::apply_on_creation(inner, &project, git_config).await?;

    let root = inner.ws.project_root_path(&project);
    let unborn = blocking({
        let root = root.clone();
        move || Ok(git::head(&root).is_err())
    })
    .await?;
    if unborn {
        let message = format!("project {} initialised", project.slug);
        match empty_root_commit(inner, &project, &message).await {
            Ok(()) | Err(EngineError::IdentityUnset { .. }) => {}
            Err(e) => tracing::warn!(project = %project.slug, "the root commit failed: {e}"),
        }
    }

    // The rail reads the primary's standing from the record, cached; the Git
    // panel reads the disk. Both must say the same thing the moment this returns.
    let primary = inner
        .ws
        .get_workstream(WorkstreamId::primary_of(project.id))?;
    inner.ide_status.invalidate(primary.id);
    inner.emit(EngineEvent::global(EnginePayload::ProjectChanged {
        project: project.id,
    }));
    inner.emit(workstream_event(
        inner,
        &primary,
        EnginePayload::WorkstreamChanged {
            workstream: primary.id,
            state: primary.state.clone(),
        },
    ));
    tracing::info!(
        project = %project.slug,
        root = %root.display(),
        committer = ?committer,
        "a plain folder was initialised as a repository"
    );
    Ok(Initialised { project, committer })
}

/// `git clone` into the project's folder, and record the result.
///
/// Like [`init_git`], a distinct step: cloning writes a whole repository onto
/// the disk and reaches the network, so it happens on an explicit action and
/// nowhere else.
pub async fn clone(
    inner: &Inner,
    project: &Project,
    url: &str,
    depth: Option<u32>,
) -> Result<Project, EngineError> {
    let path = inner.ws.project_root_path(project);
    if path.exists() && std::fs::read_dir(&path).map(|mut d| d.next().is_some())? {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-not-empty-refusing-clone-into",
            a0 = (path.display()).to_string()
        )));
    }
    let (default_branch, remote) = blocking({
        let (path, url) = (path.clone(), url.to_string());
        move || {
            git::clone(&url, &path, depth)?;
            let branch = git::default_branch(&path)?;
            let remote = git::remote_get(&path, "origin")?;
            Ok((branch, remote))
        }
    })
    .await?;
    let mut project = project.clone();
    project.vcs = Vcs::Git {
        default_branch,
        remote,
        // Offline, from `origin` alone: which kind, which instance, which repository.
        code_host: crate::codehost::record_for(inner, &path).await,
    };
    Ok(inner.ws.update_project(project)?)
}

/// Copy an existing folder into the project's managed root, and record what
/// arrived.
///
/// The third way a project gets files, beside [`init_git`] and [`clone`], and
/// the only one whose input is somebody else's directory. Two invariants make
/// it safe to point at anything:
///
/// * **The source is read and nothing else.** [`crate::import::import_tree`]
///   copies; it has no move, no delete and no write path that touches
///   `source`. Afterwards the original is byte-identical.
/// * **The destination is a managed root.** Importing into an `External` root
///   would mean writing a whole tree into a folder the user adopted precisely
///   so that Bisa would not write into it, so it is refused here rather
///   than left to the caller to remember.
///
/// Recursion is impossible by construction and not by a check: callers
/// validate the source with `resolve_adopt_path`, which refuses anything
/// inside the workspace, and a managed destination is always inside it.
///
/// `vcs` is detected from the **copy**, the same read-only way `adopt` detects
/// it from the original — `.git` came across, so the answer is the same one.
pub async fn import(
    inner: &Inner,
    project: &Project,
    source: &std::path::Path,
    limits: crate::import::ImportLimits,
) -> Result<(Project, crate::import::ImportStats), EngineError> {
    if !matches!(project.root, ProjectRoot::Managed) {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-project-has-external-root-importing-would-write",
            a0 = (project.slug).to_string()
        )));
    }
    let dest = inner.ws.project_root_path(project);

    let (stats, vcs) = tokio::task::spawn_blocking({
        let (source, dest) = (source.to_path_buf(), dest.clone());
        move || -> Result<_, EngineError> {
            let stats = crate::import::import_tree(&source, &dest, limits)?;
            // Read-only by construction: `is_repo`, `default_branch` and
            // `remote_get` never write.
            let vcs = if git::is_repo(&dest) {
                Vcs::Git {
                    default_branch: git::default_branch(&dest).unwrap_or_else(|_| "main".into()),
                    remote: git::remote_get(&dest, "origin").ok().flatten(),
                    code_host: None,
                }
            } else {
                Vcs::None
            };
            Ok((stats, vcs))
        }
    })
    .await
    .map_err(|e| {
        EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-import-task-did-not-finish",
            e = e.to_string()
        ))
    })??;

    let mut project = project.clone();
    project.vcs = match vcs {
        Vcs::Git {
            default_branch,
            remote,
            ..
        } => Vcs::Git {
            default_branch,
            remote,
            code_host: crate::codehost::record_for(inner, &dest).await,
        },
        other => other,
    };
    Ok((inner.ws.update_project(project)?, stats))
}

// ---------------------------------------------------------------------------
// Opening a workstream
// ---------------------------------------------------------------------------

/// Where one work item will run, and what the executor must tear down.
///
/// There is always a workstream: a worktree or a copy that was
/// opened for the item, or — when the repository has no commit to branch from
/// yet — the project's **primary**, which is the root checkout itself.
pub struct Workplace {
    /// The session's working directory.
    pub cwd: PathBuf,
    /// The workstream the session stands in.
    pub workstream: Workstream,
    /// `(backend, lower, merged)` for a copy-backed workstream, in the shape
    /// `executor::cleanup_iso` takes — and the only thing that ever hands it
    /// one. `None` for a git workstream and for the project root: there
    /// the tree persists, so there is nothing to unwind and nothing to diff.
    pub iso: Option<(bisa_iso::BackendKind, PathBuf, PathBuf)>,
}

impl Workplace {
    /// Whether this workplace is a git worktree on its own branch.
    pub fn is_worktree(&self) -> bool {
        matches!(self.workstream.kind, WorkstreamKind::Worktree { .. })
    }
}

/// Open the place this work item runs in.
///
/// Three outcomes, in the order they are tried:
///
/// 1. **A git project with at least one commit** → `git worktree add -b
///    <branch> <path> <base>` under `goals/<goal>/workstreams/<id>`, on a
///    branch named by [`branch_for`], based on the project's default branch
///    (falling back to whatever HEAD points at). Re-running the same work item
///    reuses its existing worktree rather than fighting over the branch name.
///
/// 2. **A git project whose HEAD is unborn** — a folder someone just
///    `git init`'d, with no commits — → the item runs **in the project root**,
///    with no workstream at all.
///
///    There is nothing to branch from, so something has to give, and the two
///    honest options are to invent an empty initial commit or to skip the
///    workstream. This takes the second. Fabricating a commit writes a permanent,
///    pushable object into a history the user has not started yet, on a path
///    nobody asked for it on — and worse, a worktree branched from an empty
///    commit is an **empty directory**: the untracked files the user just put
///    in the project would be invisible to the agent, so the work would start
///    from nothing and quietly produce the wrong thing. Running in the project
///    root starts from what is actually there. The condition heals itself: the
///    moment the repository has a first commit, workstreams open normally.
///
///    **This is the ordinary first run, not a corner.** Creating a managed
///    project runs `git init`, and a freshly initialised repository has an
///    unborn HEAD — so the very first work item on a brand-new project takes
///    *this* branch: it runs in the primary workstream, the root itself, with
///    no branch isolation until something commits. The alternative
///    would be outcome 3, which copies the project into a throwaway tree and
///    leaves a `.patch` behind; this one leaves the work in the project, and
///    the second item onward gets a proper branch.
///
/// 3. **A non-git project** → the `bisa-iso` chain copies the project
///    root into the workstream path (`WorkstreamKind::Copy`); the item's
///    patch is captured when it settles and the copy stays for the run's
///    later steps to read, torn down when the run ends — so this is the one
///    placement whose result survives only as a `.patch` result. An item run
///    again takes up its copy where it stood. A project *recorded* as git whose
///    folder is not (yet) a repository takes this route too: the disk is the
///    authority on what git can do, not the record.
///
/// Every agent step that has a project reaches here — the step's, or the
/// goal's only one ([`project_for_step`]); a step with none runs in the
/// goal's scratch folder and never opens a workstream.
pub async fn open_workstream(
    inner: &Inner,
    spec: &WorkItemSpec,
    project: &Project,
) -> Result<Workplace, EngineError> {
    open_workstream_for(inner, project, WorkstreamRequest::for_item(spec)).await
}

/// Open a workstream of `project` for whatever `req` describes. The one
/// implementation behind a work item's placement and an IDE branch.
pub async fn open_workstream_for(
    inner: &Inner,
    project: &Project,
    req: WorkstreamRequest,
) -> Result<Workplace, EngineError> {
    // A put-away project takes no new work — the same rule a step meets, so
    // the rail, the route and the MCP door all hear one sentence.
    if project.is_archived() {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-project-archived-unarchive-before-opening-workstream",
            a0 = (project.slug).to_string()
        )));
    }
    let root = materialize(inner, project).await?;

    // A re-run of the same item belongs in the same worktree: the branch is
    // derived from the item id, so a second `worktree add` would collide with
    // the first by construction.
    let reusable = match req.work_item {
        Some(item) => reusable_workstream(inner, item, project)?,
        None => None,
    };
    if let Some(existing) = reusable {
        tracing::debug!(workstream = %existing.id, "reusing the work item's existing workstream");
        let cwd = inner.ws.checkout_in(project, &existing);
        // A copy taken up again is still a copy: its patch is captured
        // against the root when the item settles, as the first time.
        let iso = matches!(existing.kind, WorkstreamKind::Copy)
            .then(|| (bisa_iso::BackendKind::Copy, root.clone(), cwd.clone()));
        return Ok(Workplace {
            cwd,
            workstream: existing,
            iso,
        });
    }

    let is_git_repo = project.vcs.is_git() && {
        let root = root.clone();
        tokio::task::spawn_blocking(move || git::is_repo(&root))
            .await
            .unwrap_or(false)
    };

    if !is_git_repo {
        return open_copy_workstream(inner, &req, project, root).await;
    }

    // Unborn HEAD: no commits, nothing to branch from. See the note above.
    let status = blocking({
        let root = root.clone();
        move || git::status(&root)
    })
    .await?;
    if status.oid.is_none() {
        // The common shape of a first run since managed projects are
        // initialised as repositories: no commit to branch from, so no branch
        // isolation until one exists. The run stands in the **primary** — the
        // root checkout is a workstream like any other, so the
        // session is attributed and nothing downstream has a special case.
        tracing::info!(
            project = %project.slug,
            "project has no commits yet; running in its primary workstream"
        );
        return Ok(Workplace {
            cwd: root,
            workstream: inner.ws.primary_workstream(project.id)?,
            iso: None,
        });
    }

    // A pull request's head and base are the code host's facts, read before
    // anything on disk moves; every other source settles its base locally.
    let adopted = match &req.source {
        WorkstreamSource::PullRequest { number } => {
            Some(pull_request_to_adopt(inner, &root, *number).await?)
        }
        _ => None,
    };
    let base = match (&adopted, &req.base, &project.vcs) {
        (Some(pr), _, _) => pr.base.clone(),
        (None, Some(b), _) => {
            bisa_vcs::git::validate_ref("base", b)?;
            b.clone()
        }
        (None, None, Vcs::Git { default_branch, .. }) if !default_branch.is_empty() => {
            default_branch.clone()
        }
        _ => {
            blocking({
                let root = root.clone();
                move || git::default_branch(&root)
            })
            .await?
        }
    };
    if let WorkstreamSource::NewBranch {
        start: Some(start), ..
    } = &req.source
    {
        bisa_vcs::git::validate_ref("start", start)?;
    }
    // An unborn HEAD has nothing to branch from: said in words before a
    // worktree is asked for, so the answer is the repository's state and not
    // git's complaint about a reference.
    let unborn = blocking({
        let root = root.clone();
        move || -> Result<bool, bisa_vcs::VcsError> { Ok(git::head(&root).is_err()) }
    })
    .await?;
    if unborn {
        return Err(EngineError::Iso(format!(
            "project {} has no commits yet — make the first commit before opening a workstream",
            project.slug
        )));
    }

    let id = WorkstreamId::from_ulid(ulid::Ulid::from_datetime(SystemTime::now()));
    let path = workstream_path(inner, project, id);

    // The tail that keeps a derived branch unique: the item's id when there
    // is one, the workstream's own otherwise.
    let tail = req.work_item.map(|w| w.0).unwrap_or(id.0);
    let short = branch_for_ulid(&req.label, tail, false);
    let long = branch_for_ulid(&req.label, tail, true);

    // The branch as the source intends it — a typed name made safe, an
    // existing branch as it is, a tag's `from/<tag>`, a pull request's head —
    // or the derived short name. The pre-create script is told this; git
    // settles a derived name's tail below.
    let intended = intended_branch(&req.source, adopted.as_ref(), &short);
    // A new branch that already exists is neither made twice nor silently
    // checked out: the source for that is the branch itself, and saying so
    // beats a checkout the person did not ask for.
    if matches!(
        req.source,
        WorkstreamSource::NewBranch { name: Some(_), .. } | WorkstreamSource::Tag { .. }
    ) {
        let taken = blocking({
            let root = root.clone();
            move || git::branch_list(&root, None)
        })
        .await?;
        if taken.iter().any(|b| b.name == intended) {
            return Err(EngineError::Invalid(bisa_core::text!(
                "error-engine-invalid-branch-exists-open-workstream-from-branch-instead",
                intended = intended.to_string()
            )));
        }
    }
    scripts::run_phase(
        inner,
        &scripts::ScriptContext {
            project,
            project_root: &root,
            workstream: id,
            path: &path,
            branch: Some(&intended),
            base: Some(&base),
            goal: req.goal,
            work_item: req.work_item,
            agent: req.agent.as_deref(),
        },
        scripts::Phase::PreCreate,
        &root,
        scripts::ScriptPolicy::Refuse,
    )
    .await?;
    std::fs::create_dir_all(path.parent().unwrap_or(&path))?;

    let branch = blocking({
        let (root, path, base, intended) =
            (root.clone(), path.clone(), base.clone(), intended.clone());
        let source = req.source.clone();
        let head = adopted.as_ref().map(|pr| pr.head.clone());
        move || match source {
            // A name the person typed, at the start they chose or the base.
            WorkstreamSource::NewBranch {
                name: Some(_),
                start,
            } => git::worktree_add(&root, &path, &intended, start.as_deref().unwrap_or(&base))
                .map(|()| intended),
            WorkstreamSource::NewBranch { name: None, start } => {
                let start = start.as_deref().unwrap_or(&base);
                match git::worktree_add(&root, &path, &short, start) {
                    Ok(()) => Ok(short),
                    // The short tail can only lose to a ref that already
                    // exists — a leftover branch from a workstream whose
                    // record is gone. The full ULID cannot collide, so one
                    // retry settles it, and a second failure is a real error
                    // carrying git's own words.
                    Err(_) if long != short => {
                        git::worktree_add(&root, &path, &long, start).map(|()| long)
                    }
                    Err(e) => Err(e),
                }
            }
            WorkstreamSource::LocalBranch { name } => {
                git::worktree_add_existing(&root, &path, &name).map(|()| name)
            }
            WorkstreamSource::RemoteBranch { remote, name } => {
                git::fetch_branch(&root, &remote, &name)?;
                take_up_remote_branch(&root, &path, &name, &remote)
            }
            WorkstreamSource::Tag {
                name, create_at, ..
            } => {
                if let Some(at) = create_at {
                    git::tag_create(&root, &name, Some(&at))?;
                }
                git::worktree_add(&root, &path, &intended, &name).map(|()| intended)
            }
            WorkstreamSource::PullRequest { .. } => {
                let head = head.expect("a pull request source is read before the tree moves");
                git::fetch_branch(&root, "origin", &head)?;
                take_up_remote_branch(&root, &path, &head, "origin")
            }
        }
    })
    .await
    .map_err(|e| match (&req.source, &adopted) {
        // The head of a pull request from a fork lives in the fork, not on
        // origin: the fetch is what finds out, and the sentence says so.
        (WorkstreamSource::PullRequest { number }, Some(pr)) => {
            EngineError::Invalid(bisa_core::text!(
                "error-engine-invalid-pull-request-s-branch-not-origin-branch",
                number = number.to_string(),
                a0 = (pr.head).to_string(),
                e = e.to_string()
            ))
        }
        _ => e,
    })?;

    let workstream = Workstream {
        id,
        project: project.id,
        name: None,
        note: None,
        pinned: false,
        kind: WorkstreamKind::Worktree {
            branch: branch.clone(),
            base: base.clone(),
        },
        goal: req.goal,
        work_item: req.work_item,
        agent: req.agent.clone(),
        state: WorkstreamState::Open,
        created_at: now_secs(),
        board: Default::default(),
    };
    inner.ws.put_workstream(&workstream)?;
    // Born from a pull request: the record is linked before anyone reads it
    // — through the one writer, never smuggled into the birth state — and
    // the code host's cache starts clean for it.
    let workstream = match &adopted {
        Some(pr) => {
            let linked = transition(
                inner,
                &workstream,
                &WorkstreamTransition::PrAdopted {
                    number: pr.number,
                    url: pr.url.clone(),
                },
            )?;
            journal_progress(inner, &linked, "adopted pr", &branch, Some(&pr.url));
            inner.cache.invalidate_codehost(id);
            linked
        }
        None => workstream,
    };
    // The post-create script, in the checkout, once the record exists: its
    // failure is reported, never undone — the workstream is real by now.
    scripts::run_phase(
        inner,
        &scripts::ScriptContext {
            project,
            project_root: &root,
            workstream: id,
            path: &path,
            branch: Some(&branch),
            base: Some(&base),
            goal: req.goal,
            work_item: req.work_item,
            agent: req.agent.as_deref(),
        },
        scripts::Phase::PostCreate,
        &path,
        scripts::ScriptPolicy::Report,
    )
    .await?;
    emit_opened(inner, &workstream, &path, req.source.describe());
    Ok(Workplace {
        cwd: path,
        workstream,
        iso: None,
    })
}

/// The copy floor, for a project that is not a git repository.
async fn open_copy_workstream(
    inner: &Inner,
    req: &WorkstreamRequest,
    project: &Project,
    root: PathBuf,
) -> Result<Workplace, EngineError> {
    let id = WorkstreamId::from_ulid(ulid::Ulid::from_datetime(SystemTime::now()));
    let path = workstream_path(inner, project, id);
    // A copy has no branch and no base; the scripts are told as much.
    let ctx = scripts::ScriptContext {
        project,
        project_root: &root,
        workstream: id,
        path: &path,
        branch: None,
        base: None,
        goal: req.goal,
        work_item: req.work_item,
        agent: req.agent.as_deref(),
    };
    scripts::run_phase(
        inner,
        &ctx,
        scripts::Phase::PreCreate,
        &root,
        scripts::ScriptPolicy::Refuse,
    )
    .await?;
    std::fs::create_dir_all(path.parent().unwrap_or(&path))?;

    let kind = tokio::task::spawn_blocking({
        let (lower, merged) = (root.clone(), path.clone());
        move || -> Result<bisa_iso::BackendKind, EngineError> {
            let resolution = bisa_iso::resolve(None);
            let mut last = String::new();
            for kind in &resolution.candidates {
                match bisa_iso::backend(*kind).start(&lower, &merged) {
                    Ok(()) => return Ok(*kind),
                    Err(e) if e.is_unavailable() => last = e.to_string(),
                    // A backend that is there and could not do it: the disk,
                    // the permissions — the node's failure, said as one.
                    Err(e) => return Err(EngineError::of_iso(e)),
                }
            }
            Err(EngineError::Iso(format!("no isolation backend: {last}")))
        }
    })
    .await
    .map_err(|e| EngineError::IsoFailed(e.to_string()))??;

    let workstream = Workstream {
        id,
        project: project.id,
        name: None,
        note: None,
        pinned: false,
        kind: WorkstreamKind::Copy,
        goal: req.goal,
        work_item: req.work_item,
        agent: req.agent.clone(),
        state: WorkstreamState::Open,
        created_at: now_secs(),
        board: Default::default(),
    };
    inner.ws.put_workstream(&workstream)?;
    scripts::run_phase(
        inner,
        &ctx,
        scripts::Phase::PostCreate,
        &path,
        scripts::ScriptPolicy::Report,
    )
    .await?;
    emit_opened(inner, &workstream, &path, "a copy".to_string());
    Ok(Workplace {
        cwd: path.clone(),
        workstream,
        iso: Some((kind, root, path)),
    })
}

/// Where an item's work ended up, asked *after* the run, and **created**.
///
/// The executor knows the placement because it opened it; anything that comes
/// later has only the spec. The rule itself — a workstream whose checkout still
/// exists, otherwise the goal's own `work/` — belongs to the store, which is
/// also what answers `GET /placement/work_item/{id}`; a second walk here would
/// be a second answer to "where did this land". What stays is the one thing
/// the store deliberately will not do: make the directory. A command spawned
/// into a directory that is not there fails before it runs, which reads as the
/// check failing rather than as the placement being absent.
///
/// This exists because the alternative is the workspace root, and a criterion
/// like `test -s content.md` passes or fails on nothing but the directory it
/// is run in: checking the root would have asked whether the *workspace* has a
/// `content.md`, which is a different question with a coincidental answer.
pub fn placement(inner: &Inner, spec: &WorkItemSpec) -> PathBuf {
    match inner.ws.work_item_root(spec.id) {
        Ok(dir) => {
            if let Err(e) = std::fs::create_dir_all(&dir) {
                tracing::warn!(item = %spec.id, "cannot create the work directory: {e}");
            }
            dir
        }
        // An item the index has not caught up with yet still has its home on
        // the spec in hand, and that is the same fallback the store would have
        // reached for.
        Err(e) => {
            tracing::warn!(item = %spec.id, "no placement on record: {e}");
            scratch_dir(inner, &spec.home)
        }
    }
}

/// A home's own `scratch/` — a goal's, or a run of the workspace's —
/// created. A directory that does not exist is not a working directory: a
/// command spawned into one fails before it runs, which would read as the
/// check failing rather than as the placement being absent. Where an agent
/// step with no project runs, a `check` with no project, the Workflow
/// Agent's design session, and `TMPDIR`.
pub fn scratch_dir(inner: &Inner, home: &Home) -> PathBuf {
    let dir = inner.ws.paths().home(home).scratch();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::warn!(%home, "cannot create the scratch directory: {e}");
    }
    dir
}

/// Where a `check` step's command runs: **where the run's work landed** —
/// the checkout of the work item the run's latest agent step ran on (a
/// project's workstream is a worktree beside the root, so the root would
/// not see a file the agent just wrote) — else, for a goal's run, the
/// project root when the goal has exactly one project, else the run's home's
/// scratch folder. A criterion like `test -s index.html` passes or fails on
/// nothing but the directory it runs in, so the directory is the one the
/// file was written to.
pub fn check_cwd(inner: &Inner, run: &WorkflowRun) -> PathBuf {
    let latest_item = run
        .workflow
        .steps
        .iter()
        .filter_map(|step| {
            let record = run.steps.get(&step.id)?;
            record.work_item.map(|item| (record.seq, item))
        })
        .max_by_key(|(seq, _)| *seq)
        .map(|(_, item)| item);
    if let Some(item) = latest_item {
        match inner.ws.work_item_root(item) {
            Ok(dir) if dir.is_dir() => return dir,
            Ok(dir) => tracing::debug!(
                target: "bisa_engine",
                item = %item,
                "the item's checkout is gone ({}); the check runs in the project root",
                dir.display()
            ),
            Err(e) => {
                tracing::debug!(target: "bisa_engine", item = %item, "no placement for the run's item: {e}")
            }
        }
    }
    match run.scope.goal() {
        Some(goal) => goal_check_root(inner, goal),
        None => scratch_dir(inner, &run.home()),
    }
}

/// Where a `check` runs when no agent item of the run has a checkout to
/// offer: the run's project root when the goal has exactly one project, else
/// the goal's scratch folder.
pub fn goal_check_root(inner: &Inner, goal: GoalId) -> PathBuf {
    let attached = attached_ids(inner, goal).unwrap_or_default();
    match resolve_step_project(None, &attached) {
        StepProject::Single(p) => match inner.ws.get_project(p) {
            Ok(project) => inner.ws.project_root_path(&project),
            Err(_) => scratch_dir(inner, &Home::Goal { goal }),
        },
        _ => scratch_dir(inner, &Home::Goal { goal }),
    }
}

/// An existing, still-usable workstream for this item on this project: a
/// worktree, or a copy whose tree still stands — a copy stays until its run
/// ends, so an item taken up again works where it left off.
fn reusable_workstream(
    inner: &Inner,
    item: WorkItemId,
    project: &Project,
) -> Result<Option<Workstream>, EngineError> {
    let candidates = inner
        .ws
        .list_workstreams(WorkstreamFilter::WorkItem(item))?;
    Ok(candidates.into_iter().find(|w| {
        w.project == project.id
            && matches!(
                w.kind,
                WorkstreamKind::Worktree { .. } | WorkstreamKind::Copy
            )
            && !matches!(w.state, WorkstreamState::Closed)
            && inner.ws.checkout_in(project, w).is_dir()
    }))
}

/// How a project's tree comes to exist. The record is the same either way; this
/// is what happens to the folder.
#[derive(Debug, Clone)]
pub enum ProjectSource {
    /// A fresh managed folder, initialised as an empty repository.
    New,
    /// `git clone` into the managed folder.
    Clone { url: String, depth: Option<u32> },
    /// Copy a folder from `source` into the managed folder.
    Import { source: PathBuf },
    /// Adopt an existing folder in place (`new.root` is `External`); what git
    /// it has is read, never created.
    Adopt,
}

/// Everything a project is made from.
#[derive(Debug, Clone)]
pub struct NewProjectRequest {
    pub new: NewProject,
    pub source: ProjectSource,
    /// Git config for the new repository, written to its local layer key by
    /// key (schema-validated). Empty leaves the repository to inherit
    /// the global config; nothing resolving there is what asks the person.
    pub git_config: Vec<(String, String)>,
}

/// What [`create`] hands back: the project as it is after its tree exists,
/// and — for an import — what did *not* come across.
#[derive(Debug, Clone)]
pub struct Created {
    pub project: Project,
    pub imported: Option<crate::import::ImportStats>,
    /// What the committer policy did for the new repository — kept, pinned,
    /// or asked — so the creation note can say so.
    pub committer: Committer,
}

/// Create a project: the one operation behind the HTTP route, the CLI and the
/// core agent's intake op (layering rule 2).
///
/// A compound transaction with a rollback that touches records only: the
/// store writes the record and its primary workstream, the tree is made
/// (`init`, `clone`, `import` or an adopt-time check), and only then is the
/// creation announced. If the tree cannot be made the record is taken back
/// so nothing claims a folder that is not there — while whatever landed on
/// disk is left alone, because a recursive delete on an error path is how a
/// bug becomes somebody's lost work.
pub async fn create(inner: &Inner, req: NewProjectRequest) -> Result<Created, EngineError> {
    let NewProjectRequest {
        mut new,
        source,
        git_config,
    } = req;
    // Before anything is written: whoever carries it is somebody here.
    crate::ops::check_assignees(inner, &new.assignees)?;
    match &source {
        ProjectSource::Clone { url, .. } if url.trim().is_empty() => {
            return Err(EngineError::Invalid(bisa_core::text!(
                "error-engine-invalid-clone-needs-url"
            )));
        }
        ProjectSource::Adopt => {
            let ProjectRoot::External { path } = &new.root else {
                return Err(EngineError::Invalid(bisa_core::text!(
                    "error-engine-invalid-adopting-needs-path-folder-adopts"
                )));
            };
            new.vcs = detect_vcs(inner, PathBuf::from(path)).await;
        }
        _ => {}
    }
    let record = inner.ws.create_project(new)?;
    let made: Result<(Project, Option<crate::import::ImportStats>), EngineError> = match source {
        ProjectSource::New => init_git(inner, &record).await.map(|p| (p, None)),
        ProjectSource::Clone { url, depth } => clone(inner, &record, url.trim(), depth)
            .await
            .map(|p| (p, None)),
        ProjectSource::Import { source } => import(inner, &record, &source, Default::default())
            .await
            .map(|(p, stats)| (p, Some(stats))),
        ProjectSource::Adopt => materialize(inner, &record)
            .await
            .map(|_| (record.clone(), None)),
    };
    // Who commits, decided once for every path — before the creation is
    // announced, so a screen that hears `project_created` can already ask.
    let made = match made {
        Ok((project, imported)) => identity::apply_on_creation(inner, &project, git_config)
            .await
            .map(|committer| (project, imported, committer)),
        Err(e) => Err(e),
    };
    match made {
        Ok((project, imported, committer)) => {
            emit_created(inner, &project);
            Ok(Created {
                project,
                imported,
                committer,
            })
        }
        Err(e) => {
            if let Err(undo) = inner.ws.delete_project(record.id) {
                tracing::warn!(project = %record.slug, "could not take back a project whose tree failed: {undo}");
            }
            Err(e)
        }
    }
}

/// Edit a project's record — the one write path behind `PATCH /projects/{pid}`
/// and `PUT /projects/{pid}/assignees` (layering rule 2). The store refuses a
/// slug or root change and a blank name, the engine an assignee nobody
/// defined; what is accepted is announced.
pub fn update(inner: &Inner, project: Project) -> Result<Project, EngineError> {
    // Whoever carries it is somebody this workspace has: a name nobody
    // answers to is refused here, never found out when work is routed.
    crate::ops::check_assignees(inner, &project.assignees)?;
    let project = inner.ws.update_project(project)?;
    inner.emit(EngineEvent::global(EnginePayload::ProjectChanged {
        project: project.id,
    }));
    Ok(project)
}

/// Forget a project's records — the project, its primary and every other
/// workstream. Files on disk are left where they are; a caller that wants the
/// managed folder gone does that separately and says so.
pub fn forget(inner: &Inner, project: ProjectId) -> Result<(), EngineError> {
    inner.ws.delete_project(project)?;
    Ok(())
}

/// What a deletion did: whether the folder went, where it was, how many workstream records went with it.
#[derive(Clone, Debug, PartialEq, serde::Serialize, schemars::JsonSchema)]
pub struct Deleted {
    pub removed_tree: bool,
    pub path: String,
    pub workstreams_forgotten: usize,
    /// Why the folder is still there, when it was asked to go and could not:
    /// the record is forgotten either way, and the person is told the truth
    /// about the disk rather than an error over a delete that half happened.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kept: Option<String>,
}

/// Delete a project: its records, and with `tree` the whole *managed* folder
/// — tree, checkouts, notes, settings — to the OS Trash, never a hard
/// delete. An adopted folder is refused with `tree`: Bisa did not
/// create it, and deleting it is not this engine's to do. Every session in
/// it is stopped first; the worktrees are closed before the repository they
/// hang off goes (the clean script reported, not obeyed — the folder is
/// about to go whatever one checkout's script says). The one implementation
/// under `DELETE /projects/{pid}`, the CLI and a retirement.
pub async fn delete(
    inner: &Arc<Inner>,
    project: ProjectId,
    tree: bool,
) -> Result<Deleted, EngineError> {
    let p = inner.ws.get_project(project)?;
    let path = inner.ws.project_root_path(&p);
    if tree && matches!(p.root, ProjectRoot::External { .. }) {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-adopted-folder-bisa-will-not-delete-delete",
            a0 = (path.display()).to_string()
        )));
    }
    let workstreams = inner
        .ws
        .list_workstreams(WorkstreamFilter::Project(project))?;
    let wids: std::collections::HashSet<WorkstreamId> = workstreams.iter().map(|w| w.id).collect();
    crate::sessions::stop_for(inner, crate::sessions::Scope::Project(project), &wids);
    if tree {
        for w in workstreams.iter().filter(|w| !w.is_primary()) {
            if let Err(e) =
                close_workstream_with(inner, w.id, true, scripts::ScriptPolicy::Report).await
            {
                tracing::warn!(workstream = %w.id, "closing before project delete: {e}");
            }
        }
    }
    // The whole managed project directory is what `tree` removes, not just
    // `tree/`; captured before forgetting, which only removes the record. It
    // goes the way a file delete goes — `editor.delete.trash`, read at the
    // machine before the record is gone: the OS Trash by default, else unlinked.
    let project_dir = inner.ws.project_paths(&p).dir().to_path_buf();
    let disposal = crate::ide::files::Disposal::from_setting(crate::ide::files::setting_bool(
        inner,
        None,
        "editor.delete.trash",
        true,
    ));
    forget(inner, project)?;
    for w in &workstreams {
        crate::ide::index::invalidate(bisa_core::FileScope::Workstream.as_str(), &w.id.to_string());
        inner.ide_status.invalidate(w.id);
    }
    // A project's root is asked for by the project's id too.
    crate::ide::index::invalidate(
        bisa_core::FileScope::Workstream.as_str(),
        &project.to_string(),
    );
    // The record is gone from here on, so nothing below may be an error: a
    // folder that will not go is a fact the answer carries, the log keeps,
    // and every screen still hears that the project is no more.
    let mut removed_tree = false;
    let mut kept = None;
    if tree && project_dir.is_dir() {
        match disposal.remover().remove(&project_dir, true) {
            Ok(()) => {
                removed_tree = true;
                tracing::info!(%project, slug = %p.slug, path = %project_dir.display(), trash = matches!(disposal, crate::ide::files::Disposal::Trash), "a deleted project's folder went");
            }
            Err(e) => {
                tracing::warn!(%project, slug = %p.slug, path = %project_dir.display(), "a deleted project's folder could not be removed and stays: {e}");
                kept = Some(e.to_string());
            }
        }
    } else {
        tracing::info!(%project, slug = %p.slug, tree, "a project was forgotten; nothing on disk moved");
    }
    inner.emit(EngineEvent::global(EnginePayload::ProjectDeleted {
        project,
    }));
    Ok(Deleted {
        removed_tree,
        path: project_dir.display().to_string(),
        workstreams_forgotten: workstreams.len(),
        kept,
    })
}

/// Put a project away, or take it back out. Archiving stops every session
/// in it; the records and the folder stay, and the rail hides it until
/// asked. Announced as `ProjectArchived`.
pub fn archive(
    inner: &Arc<Inner>,
    project: ProjectId,
    archived: bool,
) -> Result<Project, EngineError> {
    if archived {
        let wids: std::collections::HashSet<WorkstreamId> = inner
            .ws
            .list_workstreams(WorkstreamFilter::Project(project))?
            .iter()
            .map(|w| w.id)
            .collect();
        crate::sessions::stop_for(inner, crate::sessions::Scope::Project(project), &wids);
    }
    let p = inner.ws.set_project_archived(project, archived)?;
    inner.emit(EngineEvent::global(EnginePayload::ProjectArchived {
        project,
        archived,
    }));
    Ok(p)
}

/// What a person may change about a workstream: its name, its note,
/// whether it is pinned, its due date on the Board. `None` leaves a field
/// alone; `Some(None)` on `name`, `note` or `due` clears it. Its column and
/// rank are not edits but a *placement* ([`place_workstream_card`]).
#[derive(Debug, Clone, Default)]
pub struct WorkstreamEdit {
    pub name: Option<Option<String>>,
    pub note: Option<Option<String>>,
    pub pinned: Option<bool>,
    pub due: Option<Option<bisa_core::DueDate>>,
}

/// Apply a [`WorkstreamEdit`] and announce it. Kind, project and state are
/// not edits; the store refuses them by name.
pub fn edit_workstream(
    inner: &Inner,
    id: WorkstreamId,
    edit: WorkstreamEdit,
) -> Result<Workstream, EngineError> {
    let mut w = inner.ws.get_workstream(id)?;
    if let Some(name) = edit.name {
        w.name = name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty());
    }
    if let Some(note) = edit.note {
        w.note = note.map(|n| n.trim().to_string()).filter(|n| !n.is_empty());
    }
    if let Some(pinned) = edit.pinned {
        w.pinned = pinned;
    }
    if let Some(due) = edit.due {
        w.board.due = due;
    }
    let w = inner.ws.update_workstream(&w)?;
    announce_edited(inner, &w);
    Ok(w)
}

/// A workstream's person-editable fields changed: the status cache forgets
/// it and every surface hears (the payload carries ids; a surface re-reads).
fn announce_edited(inner: &Inner, w: &Workstream) {
    inner.ide_status.invalidate(w.id);
    inner.emit(workstream_event(
        inner,
        w,
        EnginePayload::WorkstreamEdited {
            workstream: w.id,
            project: w.project,
        },
    ));
}

/// Put a card at `index` in a Board column (ide/16). The column's other
/// placed cards, by rank, are the neighbours; the rank comes from the pure
/// rule in `core/board.rs` — a number between them, or the whole column
/// renumbered when they touch — and every record that changed is written
/// and announced. A closed workstream is Archived and nothing else; the
/// column is never the lifecycle state, so nothing about the checkout moves.
pub fn place_workstream_card(
    inner: &Inner,
    id: WorkstreamId,
    column: bisa_core::BoardColumn,
    index: usize,
) -> Result<Vec<Workstream>, EngineError> {
    use bisa_core::{rank_at, BoardColumn, Placement};

    // One placement at a time: the rank is read from the column and written
    // back, and two drops arriving together would each read the same
    // neighbours and hand out the same number.
    static PLACING: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one_at_a_time = PLACING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let moved = inner.ws.get_workstream(id)?;
    if matches!(moved.state, WorkstreamState::Closed) && column != BoardColumn::Archived {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-workstream-closed-closed-workstream-archived-cannot-be",
            id = id.to_string(),
            column = column.to_string()
        )));
    }
    // The column's cards apart from the moved one, by rank; a card that is
    // in this column by its lifecycle alone has no rank and no say in the order.
    let mut others: Vec<Workstream> = inner
        .ws
        .list_workstreams(WorkstreamFilter::All)?
        .into_iter()
        .filter(|w| w.id != id && w.board.column == Some(column) && w.board.rank.is_some())
        .collect();
    others.sort_by_key(|w| w.board.rank.unwrap_or(u32::MAX));
    let ranks: Vec<u32> = others.iter().filter_map(|w| w.board.rank).collect();
    let index = index.min(ranks.len());
    // Dropped where it already is: nothing is written and nothing announced.
    // A rank is the gap between two neighbours, and a card set down in its
    // own place a few times would halve that gap to nothing and renumber a
    // column nobody reordered.
    if moved.board.column == Some(column) {
        if let Some(own) = moved.board.rank {
            if ranks.iter().filter(|r| **r < own).count() == index {
                return Ok(Vec::new());
            }
        }
    }

    let mut written = Vec::new();
    match rank_at(&ranks, index) {
        Placement::Rank(rank) => {
            let mut w = moved;
            w.board.column = Some(column);
            w.board.rank = Some(rank);
            written.push(inner.ws.update_workstream(&w)?);
        }
        Placement::Renumber(fresh) => {
            let mut ordered = others;
            ordered.insert(index, moved);
            for (mut w, rank) in ordered.into_iter().zip(fresh) {
                w.board.column = Some(column);
                w.board.rank = Some(rank);
                written.push(inner.ws.update_workstream(&w)?);
            }
        }
    }
    for w in &written {
        announce_edited(inner, w);
    }
    Ok(written)
}

/// Read what git a folder already has. Read-only by construction: `is_repo`,
/// `default_branch` and `remote_get` never write, and the code host is read
/// from `origin` alone.
pub async fn detect_vcs(inner: &Inner, path: PathBuf) -> Vcs {
    let facts = tokio::task::spawn_blocking({
        let path = path.clone();
        move || {
            if !git::is_repo(&path) {
                return None;
            }
            Some((
                git::default_branch(&path).unwrap_or_else(|_| "main".into()),
                git::remote_get(&path, "origin").ok().flatten(),
            ))
        }
    })
    .await
    .ok()
    .flatten();
    match facts {
        Some((default_branch, remote)) => Vcs::Git {
            default_branch,
            remote,
            code_host: crate::codehost::record_for(inner, &path).await,
        },
        None => Vcs::None,
    }
}

/// Say on the bus that a project now exists.
///
/// Called by every path that creates one — the HTTP route, the CLI, the core
/// agent's intake op — because a surface can only refresh on a fact it is
/// told: without it, a project an agent had just made would appear only when
/// someone navigated away and back.
///
/// Not folded into [`materialize`]: that runs on the workstream path too, and
/// an event announcing a creation every time a work item opens would be a
/// louder lie than the silence it replaced.
pub(crate) fn emit_created(inner: &Inner, project: &Project) {
    inner.emit(EngineEvent::global(EnginePayload::ProjectCreated {
        project: project.id,
        slug: project.slug.to_string(),
        origin: project.origin.clone(),
    }));
}

/// An event about a workstream is scoped to the goal it was made for, when it
/// was made for one; an IDE workstream's events are global.
/// What a workstream's event is about: the run its work item serves, when it
/// serves one — so a commit a run made carries the run, and a listener that
/// hears it reads the run's causal chain — else its goal, else nobody.
fn workstream_event(inner: &Inner, w: &Workstream, payload: EnginePayload) -> EngineEvent {
    let run = w
        .work_item
        .and_then(|item| {
            let home = inner.ws.home_of_work_item(item).ok()?;
            inner.ws.get_work_item(&home, item).ok()?.run
        })
        .and_then(|run| inner.ws.get_run(run).ok());
    match (run, w.goal) {
        (Some(run), _) => EngineEvent::of_run(&run, w.work_item, payload),
        (None, Some(goal)) => EngineEvent::scoped(goal, w.work_item, payload),
        (None, None) => EngineEvent::global(payload),
    }
}

/// The branch a source intends the checkout to stand on, before git runs: a
/// typed name made safe the way a derived one is, an existing branch as it
/// is, a tag's `from/<tag>` (or the branch a person named for it), a pull
/// request's head — or `derived`, the short name a label makes.
fn intended_branch(
    source: &WorkstreamSource,
    adopted: Option<&bisa_codehost::PullRequest>,
    derived: &str,
) -> String {
    match source {
        WorkstreamSource::NewBranch { name: Some(n), .. } => typed_branch_name(n),
        WorkstreamSource::NewBranch { name: None, .. } => derived.to_string(),
        WorkstreamSource::LocalBranch { name } | WorkstreamSource::RemoteBranch { name, .. } => {
            name.clone()
        }
        WorkstreamSource::Tag {
            branch: Some(b), ..
        } => typed_branch_name(b),
        WorkstreamSource::Tag {
            name, branch: None, ..
        } => bisa_core::branch_name_for("from", name),
        WorkstreamSource::PullRequest { .. } => {
            adopted.map(|pr| pr.head.clone()).unwrap_or_default()
        }
    }
}

/// Take a remote's branch up on its local twin: the local branch of that
/// name when there is one already (checked out as it is), else a new one
/// tracking the remote's. The remote-tracking ref was fetched just before.
fn take_up_remote_branch(
    root: &Path,
    path: &Path,
    name: &str,
    remote: &str,
) -> Result<String, VcsError> {
    let local = git::branch_list(root, None)?.iter().any(|b| b.name == name);
    if local {
        git::worktree_add_existing(root, path, name)?;
    } else {
        git::worktree_add_tracking(root, path, name, remote, name)?;
    }
    Ok(name.to_string())
}

/// The pull request a workstream is opened from, read from the code host
/// behind the project's `origin` — and refused unless it is open: a closed
/// or merged one has no branch to work on.
async fn pull_request_to_adopt(
    inner: &Inner,
    root: &Path,
    number: u64,
) -> Result<bisa_codehost::PullRequest, EngineError> {
    let (host, repo) = crate::codehost::code_host_for_path(inner, root.to_path_buf()).await?;
    let pr = host.get_pr(&repo, number).await?;
    if pr.state != bisa_codehost::PrState::Open {
        return Err(EngineError::PullRequestNotOpen {
            number,
            state: pr.state.as_str().to_string(),
        });
    }
    Ok(pr)
}

fn emit_opened(inner: &Inner, w: &Workstream, checkout: &std::path::Path, source: String) {
    inner.emit(workstream_event(
        inner,
        w,
        EnginePayload::WorkstreamOpened {
            workstream: w.id,
            project: w.project,
            path: checkout.display().to_string(),
            branch: w.branch().map(str::to_string),
            source,
        },
    ));
}

/// The branch and base of a worktree workstream, or a typed refusal: the
/// primary has no branch *of its own* (it is on whatever the root checkout is
/// on) and a copy has none at all. What needs this is what only a branch cut
/// for one piece of work can do — a pull request, a forced push.
pub(crate) fn require_worktree(w: &Workstream) -> Result<(String, String), EngineError> {
    let (branch, base) = w.require_own_branch().map_err(|e| {
        EngineError::Invalid(bisa_core::text!(
            "error-engine-projects-refused",
            detail = e.to_string()
        ))
    })?;
    Ok((branch.to_string(), base.to_string()))
}

/// A workstream, its project and its checkout on disk — the resolution every
/// git operation starts from. A copy workstream resolves like any other; the
/// caller decides whether git makes sense there.
pub(crate) fn checkout_of(
    inner: &Inner,
    id: WorkstreamId,
) -> Result<(Workstream, Project, PathBuf), EngineError> {
    let w = inner.ws.get_workstream(id)?;
    let project = inner.ws.get_project(w.project)?;
    let path = inner.ws.checkout_in(&project, &w);
    Ok((w, project, path))
}

/// [`checkout_of`] for callers that only need somewhere to run git — and that
/// need it to exist.
pub(crate) fn checkout_tree(
    inner: &Inner,
    id: WorkstreamId,
) -> Result<(Project, PathBuf), EngineError> {
    let (w, project, path) = checkout_of(inner, id)?;
    if !path.is_dir() {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-workstream-project-has-no-checkout",
            a0 = (w.id).to_string(),
            a1 = (project.slug).to_string(),
            a2 = (path.display()).to_string()
        )));
    }
    Ok((project, path))
}

/// The branch a workstream's checkout is on right now: the recorded branch of
/// a worktree, the live HEAD of the primary. `None` for a detached HEAD.
fn live_branch(w: &Workstream, status: &bisa_vcs::git::Status) -> Option<String> {
    w.branch()
        .map(str::to_string)
        .or_else(|| status.branch.clone())
}

// ---------------------------------------------------------------------------
// The rest of the lifecycle
// ---------------------------------------------------------------------------

/// Record a workstream's new state and tell the bus about it.
pub(crate) fn transition(
    inner: &Inner,
    w: &Workstream,
    t: &WorkstreamTransition,
) -> Result<Workstream, EngineError> {
    let updated = inner.ws.transition_workstream(w.id, t)?;
    inner.emit(workstream_event(
        inner,
        w,
        EnginePayload::WorkstreamChanged {
            workstream: w.id,
            state: updated.state.clone(),
        },
    ));
    Ok(updated)
}

/// Journal what a workstream did, as a fact that syncs.
///
/// A workstream is local state and never leaves this machine, but "the branch
/// was committed / pushed / turned into PR #12" is true everywhere, so it goes
/// into its home's journal — its goal's, or the run of the workspace's whose
/// item opened it — as an ordinary `KIND_PROGRESS` fact — no new kind, and it
/// shows up in Pulse and verifies like anything else. A workstream with no
/// work item (opened by hand from the CLI) has no item to attribute the
/// progress to, so it journals the same sentence as a note.
pub(crate) fn journal_progress(
    inner: &Inner,
    w: &Workstream,
    verb: &str,
    object: &str,
    outcome: Option<&str>,
) {
    // A workstream made in the IDE has no home, so there is no journal to
    // write to; the event on the bus is its record.
    let home = match (w.goal, w.work_item) {
        (Some(goal), _) => Some(Home::Goal { goal }),
        (None, Some(item)) => inner.ws.home_of_work_item(item).ok(),
        (None, None) => None,
    };
    let Some(home) = home else {
        tracing::debug!(workstream = %w.id, "{verb} {object}: no goal or run to journal to");
        return;
    };
    journal_fact(
        inner,
        home,
        w.work_item,
        w.agent.as_deref(),
        verb,
        object,
        outcome,
    );
}

/// The same fact, for the callers that have a home but no workstream.
///
/// [`JournalPayload::Progress`] carries a `WorkItemId` and is not optional, so
/// "the owner committed three files in project `storefront`" — which belongs
/// to a project, not to any item — journals the identical sentence as a note.
/// That is the fallback the workstream path has always used for a hand-opened
/// checkout; a project commit is the same shape of fact and gets the same
/// treatment rather than a synthesised work item to hang it on.
pub(crate) fn journal_fact(
    inner: &Inner,
    home: Home,
    work_item: Option<WorkItemId>,
    agent: Option<&str>,
    verb: &str,
    object: &str,
    outcome: Option<&str>,
) {
    let (signer, attestation) = crate::ops::signer_for(&inner.ws, agent);
    let payload = match work_item {
        Some(work_item) => JournalPayload::Progress {
            work_item,
            verb: verb.to_string(),
            object: object.to_string(),
            outcome: outcome.map(str::to_string),
        },
        None => JournalPayload::Note {
            text: match outcome {
                Some(o) => format!("{verb} {object} → {o}"),
                None => format!("{verb} {object}"),
            },
        },
    };
    if let Err(e) = inner
        .ws
        .append_journal(&home, payload, &signer, attestation)
    {
        tracing::warn!(%home, "journalling {verb} failed: {e}");
    }
}

/// Who commits here, or a refusal that names the fix.
///
/// Asked before any commit the platform makes — a person's from the Changes
/// view, an agent's settlement — so a repository with no identity refuses by
/// name instead of failing inside git. The platform never writes global
/// config; the fix is the repository's local identity
/// ([`crate::ide::git::set_identity`]). A refusal also puts the project on
/// the committer desk with `reason` — and, for a settlement, the commit to
/// make once someone answers — so the question reaches a person.
pub(crate) async fn ensure_identity(
    inner: &Inner,
    path: PathBuf,
    project: &Project,
    reason: CommitterReason,
    retry: Option<CommitRetry>,
) -> Result<(), EngineError> {
    let git = inner.git();
    let identity = blocking(move || git.identity(&path)).await?;
    if identity.source == bisa_vcs::IdentitySource::None {
        identity::ask(inner, project, reason, retry).await;
        return Err(EngineError::IdentityUnset {
            project: project.slug.to_string(),
        });
    }
    Ok(())
}

/// Stage everything and commit — a person's commit of a whole workstream
/// (`POST /workstreams/{wid}/git/commit`, `project commit`).
///
/// Refuses cleanly rather than pretending: a clean tree gets
/// [`EngineError::NothingToCommit`] instead of an empty commit, and a
/// repository nobody is set to commit in gets [`EngineError::IdentityUnset`]
/// — the fix is the repository's local identity, never a global write.
pub async fn commit_workstream(
    inner: &Inner,
    id: WorkstreamId,
    message: &str,
) -> Result<CommitId, EngineError> {
    commit_workstream_as(inner, id, message, CommitterReason::CommitRefused, None).await
}

/// [`commit_workstream`], saying why a refused identity is being asked for
/// and what to commit once it is answered.
async fn commit_workstream_as(
    inner: &Inner,
    id: WorkstreamId,
    message: &str,
    reason: CommitterReason,
    retry: Option<CommitRetry>,
) -> Result<CommitId, EngineError> {
    let (w, project, path) = checkout_of(inner, id)?;
    if matches!(w.kind, WorkstreamKind::Copy) {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-workstream-copy-non-git-project-there-no",
            id = id.to_string()
        )));
    }

    let status = blocking({
        let path = path.clone();
        move || git::status(&path)
    })
    .await?;
    if status.is_clean {
        return Err(EngineError::NothingToCommit(id.to_string()));
    }
    ensure_identity(inner, path.clone(), &project, reason, retry).await?;
    let branch = live_branch(&w, &status).ok_or_else(|| {
        EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-workstream-detached-head-check-out-branch-first",
            id = id.to_string()
        ))
    })?;

    let commit = blocking({
        let (git, path, message) = (inner.git(), path.clone(), message.to_string());
        move || {
            git.add_all(&path)?;
            git.commit(&path, &message, false)
        }
    })
    .await?;
    inner.ide_status.invalidate(id);

    // A record the run's end already released (closed) keeps its state: the
    // commit is on the branch, which is what the retry from the committer
    // desk is for — the work is not lost — and the fact is journalled and
    // announced like any settlement.
    if !w.state.is_terminal() {
        transition(inner, &w, &WorkstreamTransition::Committed)?;
    }
    journal_progress(inner, &w, "committed", &branch, Some(commit.short()));
    inner.emit(workstream_event(
        inner,
        &w,
        EnginePayload::WorkstreamCommitted {
            workstream: id,
            branch,
            commit: commit.to_string(),
        },
    ));
    Ok(commit)
}

// ---------------------------------------------------------------------------
// A project's own working tree
// ---------------------------------------------------------------------------

/// A workstream's project and checkout — for the node's read-only routes that
/// hand a path to the vcs crate. The checkout must exist.
pub fn checkout_tree_pub(
    inner: &Inner,
    id: WorkstreamId,
) -> Result<(Project, PathBuf), EngineError> {
    checkout_tree(inner, id)
}

/// The folder a project's git calls run in.
///
/// Whether it is *a repository* is left to git to answer, at the call: a
/// project recorded as git whose folder is not one comes back as
/// [`VcsError::NotARepository`] naming the path, which is a better sentence
/// than anything this function could compose from the record.
pub(crate) fn project_tree(
    inner: &Inner,
    id: ProjectId,
) -> Result<(Project, PathBuf), EngineError> {
    let project = inner.ws.get_project(id)?;
    let path = inner.ws.project_root_path(&project);
    if !path.is_dir() {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-project-has-no-folder",
            a0 = (project.slug).to_string(),
            a1 = (path.display()).to_string()
        )));
    }
    Ok((project, path))
}

/// Say a git operation that did not go through, once, where it failed: the
/// checkout and the operation as fields, the words as git's — which
/// `bisa-vcs` has already scrubbed of any remote's credentials. What git
/// refuses as a matter of course — a conflict, a pull that is not a
/// fast-forward, a dirty tree, nothing to stash, a stash that moved, an
/// argument it would not take — is the person's to read and is said quietly;
/// a command that failed, timed out, or a git that is not there is the
/// machine's, and is a warning.
pub(crate) fn log_git_failure(workstream: WorkstreamId, operation: &str, error: &EngineError) {
    use bisa_vcs::VcsError as V;
    let expected = match error {
        EngineError::Vcs(v) => matches!(
            v,
            V::Conflict { .. }
                | V::NotFastForward { .. }
                | V::Dirty { .. }
                | V::InProgress(_)
                | V::NothingToStash
                | V::NothingToDiscard
                | V::StashMoved { .. }
                | V::InvalidArg { .. }
                | V::IdentityUnset(_)
        ),
        EngineError::Invalid(_) | EngineError::Conflict(_) => true,
        _ => false,
    };
    if expected {
        tracing::info!(%workstream, operation, "a git operation was refused: {error}");
    } else {
        tracing::warn!(%workstream, operation, "a git operation failed: {error}");
    }
}

/// Put the named paths in the index, and hand back what the tree looks like
/// afterwards.
///
/// Index-only, so it needs no gate and can be undone by [`unstage_in`]
/// or by the user's own `git`. The rows come back with the call because the
/// only reason to stage is to look at the result.
pub async fn stage_in(
    inner: &Inner,
    id: WorkstreamId,
    paths: Vec<String>,
) -> Result<Vec<FileStatus>, EngineError> {
    let (_, path) = checkout_tree(inner, id)?;
    let rows = blocking(move || {
        git::stage(&path, &paths)?;
        git::status_files(&path)
    })
    .await;
    // The index is part of what the status counts: forgotten whether the
    // write landed whole or stopped half way, so the next read asks git.
    inner.ide_status.invalidate(id);
    if let Err(e) = &rows {
        log_git_failure(id, "stage", e);
    }
    rows
}

/// Take the named paths back out of the index. **The working tree is not
/// touched** — see the invariant in `bisa-vcs`.
pub async fn unstage_in(
    inner: &Inner,
    id: WorkstreamId,
    paths: Vec<String>,
) -> Result<Vec<FileStatus>, EngineError> {
    let (_, path) = checkout_tree(inner, id)?;
    let rows = blocking(move || {
        git::unstage(&path, &paths)?;
        git::status_files(&path)
    })
    .await;
    inner.ide_status.invalidate(id);
    if let Err(e) = &rows {
        log_git_failure(id, "unstage", e);
    }
    rows
}

/// Commit a workstream's checkout, staging **what the caller selected**.
///
/// The shape is [`commit_workstream`]'s — status, refuse if clean, stage,
/// commit, journal — with one deliberate difference at the staging step. A
/// workstream is a checkout Bisa opened for one work item, so everything
/// in it belongs to that item and `add_all` is honest. The primary's checkout is
/// the *user's* working tree: they may have three unrelated edits open in it,
/// and sweeping those into somebody else's commit is exactly the accident
/// per-file staging exists to prevent. So `paths` is staged and nothing else,
/// and an empty `paths` commits whatever the caller staged earlier rather
/// than quietly meaning "everything".
///
/// **No `Publish` gate.** The gate guards `push` and `pr` because those leave
/// the machine and cannot be recalled. A commit is local, and the user's own
/// git can undo it; putting a human approval in front of it would train
/// people to click through the one that matters.
///
/// The record moves to `committed` as it does for [`commit_workstream`]: the
/// Board and the lifecycle read the record, and the Changes view is where a
/// person commits.
///
/// Three refusals, all typed and all before anything is written: a clean tree
/// (nothing to commit at all), nobody set to commit ([`EngineError::IdentityUnset`]
/// — checked before staging, so the index is left as it was), and an empty
/// index after staging (the selection matched nothing that had changed).
pub async fn commit_in(
    inner: &Inner,
    id: WorkstreamId,
    message: &str,
    paths: Vec<String>,
) -> Result<CommitId, EngineError> {
    let (w, project, path) = checkout_of(inner, id)?;
    checkout_tree(inner, id)?;
    let nothing = || EngineError::NothingToCommit(format!("project {}", project.slug));

    let status = blocking({
        let path = path.clone();
        move || git::status(&path)
    })
    .await?;
    if status.is_clean {
        return Err(nothing());
    }
    ensure_identity(
        inner,
        path.clone(),
        &project,
        CommitterReason::CommitRefused,
        None,
    )
    .await?;

    if !paths.is_empty() {
        blocking({
            let path = path.clone();
            move || git::stage(&path, &paths)
        })
        .await?;
    }

    // Asked again rather than inferred from the staging call: `git add` on a
    // path that has not actually changed succeeds and stages nothing, and the
    // difference between that and a real commit is the whole question here.
    let staged = blocking({
        let path = path.clone();
        move || git::status(&path)
    })
    .await?;
    if staged.staged == 0 {
        return Err(nothing());
    }

    let commit = blocking({
        let (git, path, message) = (inner.git(), path.clone(), message.to_string());
        move || git.commit(&path, &message, false)
    })
    .await?;
    // The rail's mark and the Board read the tree as it is now, not as the
    // two-second-old cache says.
    inner.ide_status.invalidate(id);

    // A commit is a commit, whichever door made it: the record moves as the
    // workstream's own commit moves it, and the bus hears the same two
    // facts. A record already closed keeps its state; the commit is on its
    // branch.
    if !w.state.is_terminal() {
        transition(inner, &w, &WorkstreamTransition::Committed)?;
    }
    if let Some(branch) = live_branch(&w, &staged) {
        inner.emit(workstream_event(
            inner,
            &w,
            EnginePayload::WorkstreamCommitted {
                workstream: id,
                branch,
                commit: commit.to_string(),
            },
        ));
    }

    // A project belongs to nobody; every goal it is attached to learns of
    // the commit, because each of them may be waiting on it.
    for goal in inner.ws.goals_of_project(project.id)? {
        journal_fact(
            inner,
            Home::Goal { goal },
            None,
            None,
            "committed",
            &format!("{} ({} file(s))", project.slug, staged.staged),
            Some(commit.short()),
        );
    }
    Ok(commit)
}

// ---------------------------------------------------------------------------
// The commit message an agent writes
// ---------------------------------------------------------------------------

/// How long a caller waits for a suggested commit message.
///
/// Not [`crate::config::EngineConfig::guided_wake_timeout_secs`]'s ten
/// minutes: that budget is for a wake that plans a whole goal, with nobody
/// watching. This one has somebody watching a spinner in a dialog. Ninety
/// seconds is a cold harness start plus one turn on a large diff, and it sits
/// under the node's own 120-second publish wait and under every default HTTP
/// client timeout, so the route always gets to answer before the socket does.
/// Past that, an empty box the user can type into beats a wait they cannot
/// interpret.
const SUGGESTION_DEADLINE: std::time::Duration = std::time::Duration::from_secs(90);

/// How much of a patch goes to the model. A commit message is written from
/// the shape of a change, not from every line of it, and a 40 MB
/// vendored-dependency diff would cost a context window to say "update deps".
const DIFF_BUDGET: usize = 48_000;

const MESSAGE_BRIEF: &str = "\
Write the commit message for the staged change below. Reply with the message \
and nothing else — no preamble, no code fences, no explanation of what you \
did. A short imperative subject line under 72 characters; then, only if the \
change needs it, a blank line and a brief body saying why rather than what. \
Describe what the diff shows, and do not invent a motive it does not \
support.";

/// Ask an agent for a commit message for what is **staged** right now.
///
/// It suggests, and that is the whole contract: nothing on this path stages,
/// commits or writes anything. A commit carries the user's name, so the last
/// word stays theirs — and a suggestion they disagree with costs one edit,
/// where a commit they did not write costs a rewritten history.
///
/// **The core agent writes it.** Not a configurable choice and not the
/// project's assignee: the core agent is the one agent every workspace has by
/// construction, it already carries a harness and a model plan, and its
/// output is attested. A project with no agents assigned still gets a
/// suggestion, and no workspace can reach this route with nobody home.
///
/// When there is no harness on the host, no model left in the plan, or the
/// agent says nothing, this returns the error. The caller's job is to show
/// the user an empty box and that sentence — never a message the agent did
/// not write.
pub async fn suggest_commit_message(
    inner: &Inner,
    id: WorkstreamId,
) -> Result<String, EngineError> {
    let (_, path) = checkout_tree(inner, id)?;
    suggest_for_tree(inner, path, MESSAGE_BRIEF, AgentId::GENERAL).await
}

/// The suggestion for any tree — a checkout's, the notes repository's —
/// from what is **staged** in it, with the brief the caller chooses: code
/// and notes are described differently, and the path is the same.
pub(crate) async fn suggest_for_tree(
    inner: &Inner,
    path: std::path::PathBuf,
    brief: &str,
    agent: &str,
) -> Result<String, EngineError> {
    let (diff, files) = blocking(move || {
        let diff = git::diff(&path, true)?;
        let files = git::status_files(&path)?;
        Ok((diff, files))
    })
    .await?;

    if diff.trim().is_empty() {
        return Err(EngineError::NothingToCommit(
            "nothing is staged, so there is no change to describe".into(),
        ));
    }

    let staged: Vec<String> = files
        .iter()
        .filter(|f| f.is_staged())
        .map(|f| match &f.old_path {
            Some(old) => format!("- {} (was {})", f.path.display(), old.display()),
            None => format!("- {}", f.path.display()),
        })
        .collect();

    let (patch, truncated) = budgeted(&diff);
    let prompt = format!(
        "{brief}\n\nStaged files:\n{}\n\n--- staged diff ---\n{patch}{}",
        staged.join("\n"),
        if truncated {
            "\n… (diff truncated; describe the change from what is shown)"
        } else {
            ""
        }
    );

    let text = crate::ask::ask_agent_once(inner, agent, &prompt, SUGGESTION_DEADLINE).await?;

    // Models like to wrap prose in a fence even when told not to. Unwrapping
    // one here is cheaper than a second round trip, and leaves anything else
    // it said exactly as it said it.
    Ok(unfence(&text))
}

/// A patch cut to [`DIFF_BUDGET`] characters, on a character boundary, and
/// whether it was cut.
fn budgeted(diff: &str) -> (&str, bool) {
    match diff.char_indices().nth(DIFF_BUDGET) {
        Some((at, _)) => (&diff[..at], true),
        None => (diff, false),
    }
}

// ---------------------------------------------------------------------------
// The pull request an agent drafts
// ---------------------------------------------------------------------------

/// What an agent drafts for a pull request: the title and the description.
/// A draft in the person's dialog, never a pull request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequestDraft {
    pub title: String,
    pub body: String,
}

/// How many of the branch's commits go to the model, newest first: a pull
/// request is described from what it does, and the subjects past this many
/// say less than the diff already does.
const PR_COMMIT_BUDGET: usize = 50;

const PR_BRIEF: &str = "\
Write the title and the description of a pull request for the branch below, \
from its commits and its diff against the base. Reply in exactly this shape \
and nothing else — no preamble, no code fences, no labels such as \"Title:\": \
the title on the first line, under 72 characters, in the imperative, with no \
trailing period; then one blank line; then the description in Markdown — one \
or two sentences on what changes and why, then a short bulleted list of the \
notable changes. Say how to test it only if the commits or the diff show \
tests. Describe what the commits and the diff show, and do not invent a \
motive, an issue number or a result they do not support.";

/// Ask an agent for a pull request's title and description, from what the
/// workstream's branch carries beyond its base: its commits and its diff
/// (`base...HEAD`). The sibling of [`suggest_commit_message`], under the same
/// contract — it suggests, and nothing on this path pushes, opens or writes
/// anything; the session is the same read-only one ([`crate::ask`]); the core
/// agent writes it; no harness, no model or no answer is the error, and the
/// caller shows its own fields untouched and that sentence.
///
/// A branch with nothing beyond its base is refused as opening the pull
/// request would refuse it ([`EngineError::NothingToPublish`]) — before any
/// agent is asked.
pub async fn suggest_pull_request(
    inner: &Inner,
    id: WorkstreamId,
) -> Result<PullRequestDraft, EngineError> {
    let w = inner.ws.get_workstream(id)?;
    let (branch, base) = require_worktree(&w)?;
    let path = inner.ws.workstream_checkout(&w)?;
    let (commits, diff) = blocking({
        let base = base.clone();
        move || {
            let commits = git::log(&path, Some(&base), PR_COMMIT_BUDGET)?;
            let diff = git::branch_diff(&path, &base)?;
            Ok((commits, diff))
        }
    })
    .await?;
    if commits.is_empty() {
        return Err(EngineError::NothingToPublish {
            workstream: id.to_string(),
            branch,
            base,
        });
    }
    let listed: Vec<String> = commits.iter().map(|c| format!("- {}", c.subject)).collect();
    let (patch, truncated) = budgeted(&diff);
    let prompt = format!(
        "{PR_BRIEF}\n\nBranch: {branch}, into {base}\n\nCommits on the branch, newest first:\n{}{}\n\n--- diff against {base} ---\n{patch}{}",
        listed.join("\n"),
        if commits.len() == PR_COMMIT_BUDGET {
            "\n… (older commits not listed)"
        } else {
            ""
        },
        if truncated {
            "\n… (diff truncated; describe the change from what is shown)"
        } else {
            ""
        }
    );
    let text =
        crate::ask::ask_agent_once(inner, AgentId::GENERAL, &prompt, SUGGESTION_DEADLINE).await?;
    Ok(draft_of(&unfence(&text)))
}

/// The title and the description out of a reply shaped as asked — the title
/// on the first line, a blank line, the description — and out of the shapes
/// models drift into anyway: a `Title:` label, a Markdown heading or quotes
/// around the title, a `Description:` label over the body.
fn draft_of(text: &str) -> PullRequestDraft {
    let mut lines = text.lines();
    let title = lines
        .by_ref()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(clean_title)
        .unwrap_or_default();
    let rest = lines.collect::<Vec<_>>().join("\n");
    let mut body = rest.trim();
    for label in [
        "description:",
        "body:",
        "**description:**",
        "**description**",
        "## description",
        "# description",
    ] {
        // `get`, not an index: a reply may open on a character wider than a byte.
        if body
            .get(..label.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(label))
        {
            body = body[label.len()..].trim_start();
            break;
        }
    }
    PullRequestDraft {
        title,
        body: body.to_string(),
    }
}

/// A title line without what a model wraps it in.
fn clean_title(line: &str) -> String {
    let mut t = line.trim_start_matches('#').trim();
    for label in ["**title:**", "**title**:", "title:"] {
        if t.get(..label.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(label))
        {
            t = t[label.len()..].trim();
            break;
        }
    }
    let t = t
        .trim_matches(|c| matches!(c, '"' | '`' | '\'' | '*'))
        .trim();
    t.trim_end_matches('.').trim_end().to_string()
}

/// Strip a single surrounding code fence, if the whole reply is one.
fn unfence(text: &str) -> String {
    let trimmed = text.trim();
    let Some(rest) = trimmed.strip_prefix("```") else {
        return trimmed.to_string();
    };
    let Some(body) = rest.split_once('\n').map(|(_lang, body)| body) else {
        return trimmed.to_string();
    };
    match body.trim_end().strip_suffix("```") {
        Some(inner) => inner.trim().to_string(),
        None => trimmed.to_string(),
    }
}

/// Push the workstream's branch to `origin` — **through the `Publish` gate**.
///
/// See [`pass_publish_gate`]: `Manual` refuses, `Gated` waits for a human,
/// `Auto` goes straight through.
pub async fn push_workstream(inner: &Inner, id: WorkstreamId) -> Result<(), EngineError> {
    let (w, project, path) = checkout_of(inner, id)?;
    if matches!(w.kind, WorkstreamKind::Copy) {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-workstream-copy-non-git-project-there-no-2",
            id = id.to_string()
        )));
    }
    let status = blocking({
        let path = path.clone();
        move || git::status(&path)
    })
    .await?;
    let branch = live_branch(&w, &status).ok_or_else(|| {
        EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-workstream-detached-head-nothing-push",
            id = id.to_string()
        ))
    })?;
    let w = reconcile_before_publish(inner, &w, &status, &path).await?;
    let what = format!("push {branch}");
    let asked = pass_publish_gate(inner, &w, &project, &what).await?;
    let pushed = push_branch(inner, &w, &path, &branch).await.map(|_| ());
    asked.heard(inner, &w, &what, pushed)
}

/// Push `branch` to `origin`, record `Pushed` and journal it — the part of a
/// push that comes **after** the Publish gate, shared by [`push_workstream`]
/// and a pull request opened on a branch that is not on the remote yet
/// ([`crate::codehost::open_pr`]). Returns the remote's URL.
pub(crate) async fn push_branch(
    inner: &Inner,
    w: &Workstream,
    path: &Path,
    branch: &str,
) -> Result<String, EngineError> {
    let remote_url = blocking({
        let path = path.to_path_buf();
        move || git::remote_get(&path, "origin")
    })
    .await?
    .ok_or_else(|| VcsError::NoRemote(format!("{} has no origin", path.display())))?;

    blocking({
        let (path, branch) = (path.to_path_buf(), branch.to_string());
        move || git::push(&path, "origin", &branch, true)
    })
    .await?;

    transition(inner, w, &WorkstreamTransition::Pushed)?;
    journal_progress(inner, w, "pushed", branch, Some(&remote_url));
    Ok(remote_url)
}

/// Let the record catch up with what git did before an outward action.
///
/// The record moves only when this engine commits or pushes; a commit made in
/// a terminal, or a push run by hand, leaves it behind — and a push from a
/// record still at `open` would run `git push` and *then* be refused by the
/// table. So, from the checkout's own `git status`: a branch with commits
/// beyond its base (the primary: with any commit at all) whose record says
/// `open` or `dirty` is `committed`; a `committed` record whose branch has an
/// upstream it is not ahead of is `pushed`. Both are moves the table admits;
/// nothing here goes backwards, and nothing leaves the machine.
pub(crate) async fn reconcile_before_publish(
    inner: &Inner,
    w: &Workstream,
    status: &bisa_vcs::git::Status,
    path: &Path,
) -> Result<Workstream, EngineError> {
    let mut w = w.clone();
    if matches!(w.state, WorkstreamState::Open | WorkstreamState::Dirty) {
        let has_commits = match w.kind.clone() {
            WorkstreamKind::Worktree { base, .. } => {
                let path = path.to_path_buf();
                blocking(move || git::ahead_behind(&path, &base)).await?.0 > 0
            }
            WorkstreamKind::Primary => status.oid.is_some(),
            WorkstreamKind::Copy => false,
        };
        if has_commits {
            w = transition(inner, &w, &WorkstreamTransition::Committed)?;
        }
    }
    if w.state == WorkstreamState::Committed && status.upstream.is_some() && status.ahead == 0 {
        w = transition(inner, &w, &WorkstreamTransition::Pushed)?;
    }
    Ok(w)
}

/// Open a pull request for the workstream's branch — **through the `Publish`
/// gate**, for the same reason as [`push_workstream`]. The code host behind the
/// checkout's `origin` does the work ([`crate::code host`]); this keeps the old
/// two-string signature for the CLI and the node.
pub async fn open_pr(
    inner: &Inner,
    id: WorkstreamId,
    title: &str,
    body: &str,
) -> Result<crate::codehost::PullRequest, EngineError> {
    crate::codehost::open_pr(
        inner,
        id,
        crate::codehost::PrRequest {
            title: title.to_string(),
            body: body.to_string(),
            ..Default::default()
        },
    )
    .await
}

/// What a close did: the record as it stands now, and how many sessions were
/// stopped before the checkout went.
#[derive(Clone, Debug, PartialEq, serde::Serialize, schemars::JsonSchema)]
pub struct Closed {
    pub workstream: Workstream,
    pub stopped_sessions: usize,
}

/// The workstream a close may act on: its record, project and path, with
/// the primary refused first — before any session is stopped and before any
/// ref is cut, since the primary's sessions are the project's own.
pub(crate) fn closable(
    inner: &Inner,
    id: WorkstreamId,
) -> Result<(Workstream, Project, PathBuf), EngineError> {
    let (w, project, path) = checkout_of(inner, id)?;
    if w.is_primary() {
        return Err(EngineError::Invalid(bisa_store::primary_is_the_project()));
    }
    Ok((w, project, path))
}

/// Close a workstream — the door a person, the CLI and the node come through.
///
/// **Every session standing in the checkout is stopped first**
/// (`sessions::stop_workstream`): the agents the engine runs are aborted,
/// the harnesses a person opened in its terminals are ended on the roster
/// (the desktop closes their tabs on the `aborted` frame). A checkout about
/// to go must not have an agent still writing into it, and a record marked
/// closed must not have one still working on it. The primary is refused
/// before anything is stopped.
///
/// `remove_tree` is opt-in and separate from closing on purpose: an
/// uncommitted or unpushed branch is the only copy of somebody's work, so
/// forgetting the record must not be able to delete it by accident. A tree
/// that goes runs the project's clean script first, and a script that fails
/// keeps the tree ([`scripts::ScriptPolicy::Refuse`]) — a delete is
/// irreversible, and a clean-up that could not finish is a reason to look;
/// its sessions are already stopped by then, which the person consented to.
pub async fn close_workstream(
    inner: &Arc<Inner>,
    id: WorkstreamId,
    remove_tree: bool,
) -> Result<Closed, EngineError> {
    closable(inner, id)?;
    let stopped_sessions = crate::sessions::stop_workstream(inner, id);
    close_workstream_with(inner, id, remove_tree, scripts::ScriptPolicy::Refuse).await?;
    Ok(Closed {
        workstream: inner.ws.get_workstream(id)?,
        stopped_sessions,
    })
}

/// The checkout half of a close, with the clean script's failure policy
/// chosen: the clean script, the tree, the `Close` transition — and **no
/// session stopped**. [`delete`] stopped the whole project's before calling
/// this per checkout, and the executor settles a copy workstream through it
/// from inside its own session, whose real outcome is recorded after; a stop
/// here would abort that session and record `Aborted` over the truth. The
/// paths that never asked a person — a project deleted whole — report the
/// script and carry on rather than leave one checkout behind.
pub async fn close_workstream_with(
    inner: &Inner,
    id: WorkstreamId,
    remove_tree: bool,
    policy: scripts::ScriptPolicy,
) -> Result<(), EngineError> {
    let (w, project, path) = closable(inner, id)?;
    if remove_tree && path.exists() {
        let root = inner.ws.project_root_path(&project);
        let (branch, base) = match &w.kind {
            WorkstreamKind::Worktree { branch, base } => {
                (Some(branch.as_str()), Some(base.as_str()))
            }
            _ => (None, None),
        };
        scripts::run_phase(
            inner,
            &scripts::ScriptContext {
                project: &project,
                project_root: &root,
                workstream: w.id,
                path: &path,
                branch,
                base,
                goal: w.goal,
                work_item: w.work_item,
                agent: w.agent.as_deref(),
            },
            scripts::Phase::Clean,
            &path,
            policy,
        )
        .await?;
    }
    match &w.kind {
        WorkstreamKind::Primary => unreachable!("refused above"),
        WorkstreamKind::Worktree { .. } if remove_tree && path.exists() => {
            blocking(move || git::worktree_remove(&path, true)).await?;
        }
        WorkstreamKind::Copy if remove_tree && path.exists() => {
            // The copy backend is always here: a tree it could not take
            // away — the disk, the permissions — is this node's failure and
            // never *no backend on this machine*.
            tokio::task::spawn_blocking(move || {
                bisa_iso::backend(bisa_iso::BackendKind::Copy).stop(&path)
            })
            .await
            .map_err(|e| EngineError::IsoFailed(e.to_string()))?
            .map_err(EngineError::of_iso)?;
        }
        WorkstreamKind::Worktree { .. } | WorkstreamKind::Copy => {}
    }
    if !w.state.is_terminal() {
        transition(inner, &w, &WorkstreamTransition::Close)?;
    }
    // A checkout that is closed, or gone, is no longer asked about: what was
    // held for it is let go now rather than when its time happens to pass.
    crate::ide::index::invalidate(bisa_core::FileScope::Workstream.as_str(), &id.to_string());
    Ok(())
}

// ---------------------------------------------------------------------------
// The Publish gate
// ---------------------------------------------------------------------------

/// Stand between a workstream and the outside world.
///
/// | `project.publish` | what happens |
/// |---|---|
/// | `Manual`  | refused here; a person runs the push themselves |
/// | `Gated`   | a `GateKind::Publish` gate opens and this waits for a human |
/// | `Auto`    | straight through, no gate |
///
/// The gate is opened through the same [`crate::gates::Gates`] machinery as
/// every other gate and emits the same `GateOpened` event, so a pending push
/// lands in the inbox next to a contract approval rather than in a mechanism
/// of its own.
pub(crate) async fn pass_publish_gate(
    inner: &Inner,
    w: &Workstream,
    project: &Project,
    what: &str,
) -> Result<Asked, EngineError> {
    match project.publish {
        PublishPolicy::Auto => return Ok(Asked::Nobody),
        PublishPolicy::Manual => {
            return Err(EngineError::PublishManual {
                project: project.slug.to_string(),
                what: what.to_string(),
            })
        }
        PublishPolicy::Gated => {}
    }
    // The gate is asked in the workstream's home: its goal, or — a
    // workstream a run of the workspace's item opened — that run. One with
    // neither has nobody to ask on the engine's bus, so publishing it is a
    // person's action, taken where the workstream was made.
    let home = match (w.goal, w.work_item) {
        (Some(goal), _) => Some(Home::Goal { goal }),
        (None, Some(item)) => inner.ws.home_of_work_item(item).ok(),
        (None, None) => None,
    };
    let Some(home) = home else {
        return Err(EngineError::PublishNoGoal {
            project: project.slug.to_string(),
            workstream: w.id.to_string(),
            what: what.to_string(),
        });
    };
    let question = format!("Publish: {what} from project {}?", project.slug);
    let subject = format!("workstream:{}", w.id);
    let (gate_id, rx) = inner.gates.open(
        home,
        w.work_item,
        Gate::Publish,
        subject.clone(),
        question.clone(),
        AskKind::Decision,
    );
    // The fact behind the gate: a restart that finds this asker dead
    // withdraws the question by its subject and says so on the home.
    crate::ops::journal_question(
        inner,
        home,
        w.work_item,
        &subject,
        &question,
        AskKind::Decision,
    );
    inner.emit(inner.home_scope(&home).event(
        w.work_item,
        EnginePayload::GateOpened {
            gate_id,
            gate: Gate::Publish,
            question,
        },
    ));
    if inner.gates.wait(rx).await.approve {
        Ok(Asked::APerson)
    } else {
        Err(EngineError::PublishDeclined {
            what: what.to_string(),
        })
    }
}

/// Whether a person was asked before an outward act — what decides who
/// hears how it ended ([`Asked::heard`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Asked {
    /// The project publishes by itself: whoever called is waiting on the
    /// call, and hears its end from it.
    Nobody,
    /// A person said yes at the gate. Whoever called was answered when the
    /// gate opened — nothing had left the machine then — and is gone.
    APerson,
}

impl Asked {
    /// The end of the act that followed the gate, handed on as it is — and
    /// **said**, when it failed after a person approved it: a fact of the
    /// workstream on the bus and in the feed (`WorkstreamPublishFailed`) and
    /// a line of its home's journal. Without it an approval that came to
    /// nothing looked, to the person who gave it, like an approval nobody
    /// acted on.
    pub(crate) fn heard<T>(
        self,
        inner: &Inner,
        w: &Workstream,
        what: &str,
        end: Result<T, EngineError>,
    ) -> Result<T, EngineError> {
        if let (Asked::APerson, Err(e)) = (self, &end) {
            publish_failed(inner, w, what, e);
        }
        end
    }
}

fn publish_failed(inner: &Inner, w: &Workstream, what: &str, error: &EngineError) {
    // A remote's refusal can quote the address it was reached at, and an
    // address can hold a credential: what is said is redacted first.
    let reason = inner.security.redact_inbound(&error.to_string(), "publish");
    tracing::warn!(
        target: "bisa_engine::projects",
        workstream = %w.id,
        project = %w.project,
        what,
        "an approved publish did not go out: {reason}"
    );
    journal_progress(inner, w, "could not", what, Some(&reason));
    let scope = match (w.goal, w.work_item) {
        (Some(goal), _) => inner.home_scope(&Home::Goal { goal }),
        (None, Some(item)) => inner
            .ws
            .home_of_work_item(item)
            .map(|home| inner.home_scope(&home))
            .unwrap_or_default(),
        (None, None) => crate::events::EventScope::default(),
    };
    inner.emit(scope.event(
        w.work_item,
        EnginePayload::WorkstreamPublishFailed {
            workstream: w.id,
            project: w.project,
            what: what.to_string(),
            reason,
        },
    ));
}

// ---------------------------------------------------------------------------
// Executor helpers
// ---------------------------------------------------------------------------

/// The ids of the projects attached to a goal.
pub fn attached_ids(inner: &Inner, goal: GoalId) -> Result<Vec<ProjectId>, EngineError> {
    Ok(inner
        .ws
        .projects_for(goal)?
        .into_iter()
        .map(|p| p.id)
        .collect())
}

/// The core's placement rule over this goal's attachments, with ambiguity as
/// the typed refusal ([`EngineError::ProjectAmbiguous`]).
pub fn resolve_for_step(
    inner: &Inner,
    goal: GoalId,
    named: Option<ProjectId>,
    step: &StepId,
) -> Result<StepProject, EngineError> {
    let attached = attached_ids(inner, goal)?;
    match resolve_step_project(named, &attached) {
        StepProject::Ambiguous { count } => Err(EngineError::ProjectAmbiguous {
            step: step.clone(),
            count,
        }),
        resolved => Ok(resolved),
    }
}

/// Refuse a run before it starts when any agent step names no project and the
/// goal has several: the person names one with a `project` input, rather than
/// the engine guessing which repository the work belongs in.
pub fn refuse_ambiguous_steps(
    inner: &Inner,
    goal: GoalId,
    workflow: &bisa_core::Workflow,
) -> Result<(), EngineError> {
    let attached = attached_ids(inner, goal)?;
    for step in &workflow.steps {
        if let StepKind::Agent { project: None, .. } = &step.kind {
            if let StepProject::Ambiguous { count } = resolve_step_project(None, &attached) {
                return Err(EngineError::ProjectAmbiguous {
                    step: step.id.clone(),
                    count,
                });
            }
        }
    }
    Ok(())
}

/// Attach a project to a goal and say so: the bus for the screens, a journal
/// note for whoever reads the goal later. Shared by the engine's own project
/// creation and an agent's `create_project`.
pub(crate) fn attach_and_journal(
    inner: &Inner,
    goal: GoalId,
    project: &Project,
    by: Option<&str>,
    committer: &Committer,
) -> Result<(), EngineError> {
    inner.ws.attach(goal, project.id)?;
    inner.emit(EngineEvent::scoped(
        goal,
        None,
        EnginePayload::AttachmentChanged {
            project: project.id,
            attached: true,
        },
    ));
    let text = match project.origin.step_ref() {
        Some(by) => format!(
            "project {:?} created (git) for the goal by step `{}` and attached",
            project.slug.as_str(),
            by.step
        ),
        None => format!(
            "project {:?} created (git) and attached",
            project.slug.as_str()
        ),
    };
    crate::ops::journal_note_as(
        inner,
        by,
        Home::Goal { goal },
        with_committer(text, committer),
    );
    Ok(())
}

/// A creation note, ending with what the committer policy did when it did
/// anything: "…and attached; commits by Ada <ada@…>, pinned from the
/// machine's default".
pub(crate) fn with_committer(text: String, committer: &Committer) -> String {
    match committer.sentence() {
        Some(clause) => format!("{text}; {clause}"),
        None => text,
    }
}

/// The one path a project is made by from an agent's `create_project`: a
/// managed git repository, one root commit so its first workstream is a
/// branch of its own, attached to the goal and journaled when there is one.
/// No step and no run makes a project; an agent asked for files that must
/// be kept does, once. From a channel or a direct message there is no goal:
/// the project is made all the same, attached to nothing.
pub async fn create_for_agent(
    inner: &Inner,
    new: NewProject,
    goal: Option<GoalId>,
    by: Option<&str>,
) -> Result<Created, EngineError> {
    let created = create(
        inner,
        NewProjectRequest {
            new,
            source: ProjectSource::New,
            git_config: Vec::new(),
        },
    )
    .await?;
    root_commit(inner, goal, &created.project, by).await;
    if let Some(goal) = goal {
        attach_and_journal(inner, goal, &created.project, by, &created.committer)?;
    }
    Ok(created)
}

/// The empty root commit a project an agent makes starts with. Without it the
/// repository's HEAD is unborn, the first step runs in the primary and nothing
/// commits; with it the first step gets a worktree on its own branch
/// like every step after it. Needs a commit identity like any commit: when the
/// repository has none, the commit is skipped and — when there is a goal to
/// tell — a note says so; the step still runs, in the primary, and the person
/// sets who commits (I45).
async fn root_commit(inner: &Inner, goal: Option<GoalId>, project: &Project, by: Option<&str>) {
    let message = match goal {
        Some(goal) => format!("project {} created for goal {goal}", project.slug),
        None => format!("project {} created", project.slug),
    };
    match empty_root_commit(inner, project, &message).await {
        Ok(()) => {}
        Err(e @ EngineError::IdentityUnset { .. }) => {
            if let Some(goal) = goal {
                crate::ops::journal_note_as(
                    inner,
                    by,
                    Home::Goal { goal },
                    format!(
                        "project {:?} has no commit identity, so it starts without a root commit: its first step runs in the primary and nothing commits until who commits is set ({e})",
                        project.slug.as_str()
                    ),
                );
            }
        }
        Err(e) => tracing::warn!(project = %project.slug, "the root commit failed: {e}"),
    }
}

/// The one empty commit that gives a fresh repository a first ancestor, shared
/// by an agent's `create_project` ([`root_commit`]) and a person's
/// [`init_repository`]. Stops at [`EngineError::IdentityUnset`] — the ask
/// raised, nothing written — when nobody resolves; the caller decides what to
/// say about that.
async fn empty_root_commit(
    inner: &Inner,
    project: &Project,
    message: &str,
) -> Result<(), EngineError> {
    let root = inner.ws.project_root_path(project);
    ensure_identity(inner, root.clone(), project, CommitterReason::Created, None).await?;
    let git = inner.git();
    let message = message.to_string();
    blocking(move || git.commit(&root, &message, true)).await?;
    Ok(())
}

/// The project a work item runs in, when it has one: the step's when it
/// names one, the goal's only project when it names none — a run of the
/// workspace attaches nothing, so its item runs in the project it names or
/// none. `None` is the home's scratch folder, and nothing is made here: no
/// project is ever born of a step. The item is updated to carry the project
/// it ended up in.
pub async fn project_for_step(
    inner: &Inner,
    spec: &mut WorkItemSpec,
) -> Result<Option<Project>, EngineError> {
    let attached = match spec.home.goal() {
        Some(goal) => attached_ids(inner, goal)?,
        None => Vec::new(),
    };
    let project = match resolve_step_project(spec.project, &attached) {
        StepProject::Ambiguous { count } => {
            return Err(match spec.step.clone() {
                Some(step) => EngineError::ProjectAmbiguous { step, count },
                None => EngineError::Invalid(bisa_core::text!(
                    "error-engine-invalid-work-item-names-no-project-goal-has",
                    a0 = (spec.id).to_string(),
                    count = count.to_string()
                )),
            })
        }
        StepProject::Scratch => return Ok(None),
        StepProject::Named(p) | StepProject::Single(p) => {
            let project = inner.ws.get_project(p)?;
            if project.is_archived() {
                return Err(EngineError::Invalid(bisa_core::text!(
                    "error-engine-invalid-project-archived-unarchive-before-step-runs",
                    a0 = (project.slug).to_string()
                )));
            }
            project
        }
    };
    if spec.project != Some(project.id) {
        spec.project = Some(project.id);
        inner.ws.put_work_item(spec)?;
    }
    Ok(Some(project))
}

/// A spawned goal works where its parent works: every project attached to
/// the parent is attached to the child — additive, and the store's `attach`
/// is idempotent and journals the fact — so a sub-goal never has to mint a
/// project of its own. Called from `ops::submit`, before anything can run on
/// the child, so placement resolves there the way it would on the parent.
pub(crate) fn inherit_attachments(
    inner: &Inner,
    parent: GoalId,
    child: GoalId,
) -> Result<(), EngineError> {
    for project in inner.ws.projects_for(parent)? {
        if project.is_archived() {
            continue;
        }
        inner.ws.attach(child, project.id)?;
        inner.emit(EngineEvent::scoped(
            child,
            None,
            EnginePayload::AttachmentChanged {
                project: project.id,
                attached: true,
            },
        ));
    }
    Ok(())
}

/// A commit message for work the executor is settling: the first line of the
/// instructions, with the item id so the commit points back at its work item.
pub fn settlement_message(spec: &WorkItemSpec) -> String {
    let head = spec
        .instructions
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("work")
        .trim();
    let head: String = head.chars().take(72).collect();
    format!("{head}\n\nbisa-work-item: {}\n", spec.id)
}

/// What settling a git workstream came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Settlement {
    /// The session's work is a commit on the branch.
    Committed,
    /// The session left the tree as it found it: nothing to keep.
    Clean,
    /// A commit was owed and refused — no identity, a hook said no; the
    /// tree is exactly as left and the reason is on the goal.
    Refused,
}

/// Commit whatever the session left behind in a git workstream, if anything.
///
/// Called on settlement. A clean tree is the ordinary case for a work item
/// that only read or answered — the executor then closes the worktree, since
/// there is nothing in it to keep. **Nothing is pushed here** — that is the
/// `Publish` gate, a separate and deliberate action.
pub async fn commit_on_settle(inner: &Inner, spec: &WorkItemSpec, id: WorkstreamId) -> Settlement {
    settle_commit(inner, id, &settlement_message(spec)).await
}

/// A worktree whose item settled with nothing to commit: the tree goes, its
/// branch goes, the record closes — and no session is stopped, because this
/// runs inside the session's own settlement, like a copy's close. A branch
/// that carries a commit of its own — a session that committed by hand — is
/// not a clean branch and is kept; so is one whose state a person already
/// moved. No journal line: a read-only step leaving nothing behind is not
/// an event.
pub async fn close_clean_worktree(inner: &Inner, w: &Workstream) {
    let WorkstreamKind::Worktree { branch, base } = &w.kind else {
        return;
    };
    if w.state != WorkstreamState::Open {
        return;
    }
    let Ok((_, project, checkout)) = checkout_of(inner, w.id) else {
        return;
    };
    let ahead = {
        let checkout = checkout.clone();
        let base = base.clone();
        blocking(move || git::ahead_behind(&checkout, &base)).await
    };
    match ahead {
        Ok((0, _)) => {}
        Ok((n, _)) => {
            tracing::debug!(workstream = %w.id, "branch carries {n} commit(s) of its own; kept");
            return;
        }
        Err(e) => {
            tracing::warn!(workstream = %w.id, "cannot tell whether the branch moved; kept: {e}");
            return;
        }
    }
    if let Err(e) = close_workstream_with(inner, w.id, true, scripts::ScriptPolicy::Report).await {
        tracing::warn!(workstream = %w.id, "closing a clean worktree failed: {e}");
        return;
    }
    let root = inner.ws.project_root_path(&project);
    let name = branch.clone();
    if let Err(e) = blocking(move || git::branch_delete(&root, &name)).await {
        tracing::warn!(workstream = %w.id, "deleting the clean branch failed: {e}");
    }
    tracing::info!(workstream = %w.id, "closed: the session left nothing to keep");
}

/// The settlement commit itself, with the message it is meant to carry —
/// made on settlement and made again by [`crate::identity::committer_set`]
/// once a missing identity has been set. A refusal for want of an identity
/// keeps this workstream and this message on the committer desk.
pub(crate) async fn settle_commit(inner: &Inner, id: WorkstreamId, message: &str) -> Settlement {
    // The message is the work item's own words: a secret quoted there would
    // otherwise sit in the history forever.
    let message = inner.security.redact_inbound(message, "commit_message");
    let retry = CommitRetry {
        workstream: id,
        message: message.clone(),
    };
    match commit_workstream_as(
        inner,
        id,
        &message,
        CommitterReason::SettlementRefused,
        Some(retry),
    )
    .await
    {
        Ok(commit) => {
            tracing::info!(workstream = %id, "committed {}", commit.short());
            Settlement::Committed
        }
        Err(EngineError::NothingToCommit(_)) => {
            tracing::debug!(workstream = %id, "workstream is clean; nothing to commit");
            Settlement::Clean
        }
        Err(e) => {
            // A refusal here (an unset git identity, a pre-commit hook that
            // said no) must not lose the work: the worktree stays exactly as
            // the session left it, and the reason is recorded on the goal
            // so a human can act on it.
            tracing::warn!(workstream = %id, "commit failed: {e}");
            if let Ok(w) = inner.ws.get_workstream(id) {
                warn_on_err(
                    transition(inner, &w, &WorkstreamTransition::CommitRefused),
                    "marking a workstream dirty",
                );
                journal_progress(
                    inner,
                    &w,
                    "left uncommitted",
                    w.branch().unwrap_or("workstream"),
                    Some(&e.to_string()),
                );
            }
            Settlement::Refused
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pull_request_draft_is_the_first_line_and_the_rest() {
        let d = draft_of(
            "Add the cart total\n\nThe cart now shows its total.\n\n- adds `total()`\n- tests it",
        );
        assert_eq!(d.title, "Add the cart total");
        assert_eq!(
            d.body,
            "The cart now shows its total.\n\n- adds `total()`\n- tests it"
        );
    }

    #[test]
    fn a_pull_request_draft_survives_the_shapes_a_model_drifts_into() {
        let labelled =
            draft_of("Title: \"Fix rounding in the cart.\"\n\nDescription:\nRounds half up.");
        assert_eq!(labelled.title, "Fix rounding in the cart");
        assert_eq!(labelled.body, "Rounds half up.");
        let heading = draft_of("\n\n# **Fix rounding**\n\n## Description\n\nRounds half up.");
        assert_eq!(heading.title, "Fix rounding");
        assert_eq!(heading.body, "Rounds half up.");
        let fenced = draft_of(&unfence(
            "```markdown\nFix rounding\n\nRounds half up.\n```",
        ));
        assert_eq!(
            fenced,
            PullRequestDraft {
                title: "Fix rounding".into(),
                body: "Rounds half up.".into()
            }
        );
        let alone = draft_of("Fix rounding");
        assert_eq!(
            (alone.title.as_str(), alone.body.as_str()),
            ("Fix rounding", ""),
            "a title alone is a draft with no body"
        );
        assert_eq!(
            draft_of("   \n  ").title,
            "",
            "nothing said is an empty title, never a made-up one"
        );
        // A reply that opens on a character wider than a byte is read, not a panic.
        assert_eq!(
            draft_of("Résumé support\n\névite les accents cassés").body,
            "évite les accents cassés"
        );
    }

    #[test]
    fn a_typed_branch_name_is_made_safe_and_never_refused() {
        assert_eq!(typed_branch_name("feature/dark mode"), "feature/dark-mode");
        let odd = typed_branch_name("Dark Mode!");
        assert!(odd.starts_with("work/dark-mode"), "{odd}");
        assert!(
            odd.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "/-_.".contains(c)),
            "{odd}"
        );
        assert_eq!(typed_branch_name("/feature/x/"), "feature/x");
        assert!(!typed_branch_name("release/1.2..3").contains(".."));
        assert!(
            !typed_branch_name("").is_empty(),
            "an empty name falls back to a legal one"
        );
    }
}
