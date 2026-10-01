//! Reading the files a workspace owns, from outside the process that made
//! them.
//!
//! A goal owns real folders — the `work` directory its sessions run in, the
//! patches they produced, its notes — and so does a run of the workspace;
//! a project owns its tree and its workstreams. This is the read-only inspector; the IDE's write path is the
//! engine's (`docs/architecture/ide/03-files-and-editing.md`).
//!
//! Three questions, answered here and nowhere else:
//!
//! - **Where is this thing?** [`Workspace::placement`] — one root per
//!   `(scope, id)`.
//! - **What is in it?** [`Workspace::list_tree`] — a bounded listing.
//! - **What does this file say?** [`Workspace::read_file`] — a bounded read.
//!
//! # Every path comes from a caller, so every path is checked
//!
//! [`crate::paths::resolve_within`] is the boundary, and the base it is given
//! is the *scope's* root rather than the workspace's. That distinction is the
//! reason it takes a base at all: an adopted project's root
//! ([`bisa_core::ProjectRoot::External`]) is legitimately outside the
//! workspace, so checking a path under it against the workspace root would
//! refuse every legal read, and checking it against nothing would serve the
//! whole disk. The boundary for a path is the root it was reached through.
//!
//! # Annotation is the point
//!
//! An entry carries what it *is* — a snapshot, a patch, a note, the journal —
//! not just its name, so a goal's folder reads as the life of that goal rather
//! than as a directory dump, and a run's as the life of that run. The
//! classification comes from position in the layout, and the layout names are
//! taken from [`HomePaths`] rather than spelled again here.
//!
//! A project's or a workstream's root gets no annotation at all. What is in
//! there is somebody's source tree, and calling their `src/` an artifact
//! would be a lie dressed as a feature.
//!
//! # Bounds
//!
//! This is the same discipline the engine's files scan (a project start's
//! `files` change over a plain folder) already proved, for the same two
//! reasons: an unbounded walk of an adopted monorepo is a hang on a route a
//! person is waiting on, and a followed symlink is an escape from the root
//! that was just checked. So: bounded
//! depth, bounded entry count, and `symlink_metadata` everywhere, which makes
//! a symlink a leaf and a cycle impossible however the tree is arranged.
//!
//! There is deliberately **no filesystem-watch dependency** here either — no
//! `notify`, no inotify budget spent per open folder. This is a poll-shaped
//! read, called when somebody asks.

use std::collections::VecDeque;
use std::io::Read as _;
use std::path::{Path, PathBuf};

use bisa_core::{GoalId, Home, RunId, WorkItemId, WorkstreamId};
use serde::{Deserialize, Serialize};

use crate::error::StoreError;
use crate::paths::{resolve_within, HomePaths};
use crate::workspace::Workspace;

/// Deepest a listing ever goes, whatever the caller asks for.
///
/// Eight is the files scan's default ceiling (`events.files.max_depth`) and
/// it is the right one here for the same reason: it clears every layout this
/// platform creates (`workstreams/<id>/` plus a checkout is four) with room
/// for somebody's own nesting, and stops well short of a `node_modules` that
/// would otherwise cost minutes on one request.
pub const TREE_MAX_DEPTH: usize = 8;

/// How deep a listing goes when the caller does not say.
///
/// One level, the way `ls` and every file browser open: a listing is something
/// a person expands, and defaulting to the ceiling would make the cheap case
/// pay for the expensive one on every navigation.
pub const TREE_DEFAULT_DEPTH: usize = 1;

/// Most entries one listing returns.
///
/// Lower than the files scan's 5,000 on purpose — that scan reports paths to
/// a listener, this one renders in front of a person, and an entry costs
/// roughly 150 bytes on the wire. Past a couple of thousand rows nobody is
/// reading a listing, they are searching, and the honest answer is
/// `truncated`.
pub const TREE_MAX_ENTRIES: usize = 2_000;

/// Most bytes [`Workspace::read_file`] ever returns.
///
/// A quarter of the transcript route's 1 MiB per read, because a transcript is
/// consumed by a program and this is read by a person: 256 KiB is some five
/// thousand lines of source. Anything larger is a data file, and a data file
/// truncated to its first page is more useful than a refusal — hence
/// `truncated: true` rather than an error.
pub const FILE_MAX_BYTES: u64 = 256 * 1024;

// ---------------------------------------------------------------------------
// Scope
// ---------------------------------------------------------------------------

/// Which rooted thing a path is being read under — the core's, since an
/// artifact's source names one on the wire.
pub use bisa_core::FileScope;

// ---------------------------------------------------------------------------
// What comes back
// ---------------------------------------------------------------------------

/// What an entry is, rather than only what it is called.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    /// Where a goal's sessions run when the work names no project.
    Work,
    /// A note's truth file.
    Note,
    /// A work item's captured result: the patch a copy workstream left.
    Result,
    /// The signed event log — the truth the rest of the tree indexes.
    Journal,
    /// An addressable snapshot.
    State,
    /// A file a person gave the goal as context (`documents/`).
    Document,
    Dir,
    File,
}

impl EntryKind {
    /// What to call this kind in a listing. Deliberately the same word serde
    /// derives, so a column in the terminal and a field on the wire cannot
    /// come to mean different things.
    pub fn as_str(self) -> &'static str {
        match self {
            EntryKind::Work => "work",
            EntryKind::Note => "note",
            EntryKind::Result => "result",
            EntryKind::Journal => "journal",
            EntryKind::State => "state",
            EntryKind::Document => "document",
            EntryKind::Dir => "dir",
            EntryKind::File => "file",
        }
    }
}

impl std::fmt::Display for EntryKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One row of a listing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    /// Relative to the scope's root, `/`-separated — always relative to the
    /// root and never to whatever sub-path was listed, so a client can hand it
    /// straight back as `?path=`.
    pub path: String,
    pub name: String,
    pub kind: EntryKind,
    pub dir: bool,
    /// A symlink is listed and never descended into. Omitting it would make
    /// the listing lie about what is in the directory; following it would
    /// leave the root that was just checked.
    pub symlink: bool,
    /// Files only.
    pub size: Option<u64>,
    /// Seconds since the epoch, when the filesystem will say.
    pub modified: Option<u64>,
    /// Matched by the root's `.gitignore` or `.git/info/exclude`. Listed all
    /// the same — dimmed, never hidden: a build output you cannot see is one
    /// you cannot delete.
    #[serde(default)]
    pub ignored: bool,
}

/// The root's ignore rules — `.gitignore` and `.git/info/exclude` — so a
/// listing and a watcher frame can both say `ignored: true` by one rule. A
/// root that is not a repository ignores nothing.
pub fn ignore_rules(root: &Path) -> ignore::gitignore::Gitignore {
    if !root.join(".git").exists() {
        return ignore::gitignore::Gitignore::empty();
    }
    let mut b = ignore::gitignore::GitignoreBuilder::new(root);
    // A missing file is the ordinary case, not an error worth a log line.
    let _unused = b.add(root.join(".gitignore"));
    let _unused = b.add(root.join(".git").join("info").join("exclude"));
    b.build()
        .unwrap_or_else(|_| ignore::gitignore::Gitignore::empty())
}

/// A bounded listing under one root.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileTree {
    pub root: PathBuf,
    /// What was listed, relative to `root`. Empty means the root itself.
    pub path: String,
    /// How deep this listing actually went, after clamping.
    pub depth: usize,
    /// The entry budget ([`TREE_MAX_ENTRIES`]) cut this listing: entries that
    /// are *at* the depths listed are missing from `entries`. A client shows
    /// this — what is on screen is not all of it.
    pub truncated: bool,
    /// There are entries below the depth asked for. Never set for a directory
    /// that is merely empty at the depth limit — that is a complete answer.
    /// A lazy client that lists one level at a time expects this and lists
    /// the deeper level when a folder is opened; it is not a cut.
    pub deeper: bool,
    pub entries: Vec<FileEntry>,
}

/// The two bounds a walk can hit, kept apart because a client treats them
/// differently: `truncated` is shown, `deeper` is listed on demand.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Bounds {
    pub truncated: bool,
    pub deeper: bool,
}

/// Where a scope's files live, and whether anything is there yet.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Placement {
    pub path: PathBuf,
    pub exists: bool,
}

/// One file, bounded.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileContent {
    /// As asked for, relative to the root — not the absolute path, which
    /// would put the workspace's location into every response.
    pub path: String,
    /// The file's real length, which is larger than `text` when `truncated`.
    pub size: u64,
    /// No bytes are served for a binary file. There is nothing a caller could
    /// do with a JSON string of a PNG except render mojibake.
    pub binary: bool,
    pub truncated: bool,
    pub text: Option<String>,
    /// sha256, hex, of the bytes served — what a compare-and-swap write hands
    /// back as `base_hash`. Present for text that was not truncated; a
    /// truncated read cannot be written back, so it carries none.
    pub hash: Option<String>,
}

/// How much of a file a reader will take. The inspector's route stays at
/// [`FILE_MAX_BYTES`]; the editor asks for more and states its own policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadCap(pub u64);

impl ReadCap {
    pub const INSPECTOR: ReadCap = ReadCap(FILE_MAX_BYTES);
}

/// sha256 hex of some bytes — the one hash this crate uses for a guarded write.
pub fn content_hash(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}

// ---------------------------------------------------------------------------
// The resolver
// ---------------------------------------------------------------------------

impl Workspace {
    /// The absolute root a `(scope, id)` names. **The one place a scope
    /// becomes a directory.**
    ///
    /// Note what each arm returns. A goal is its own folder, and so is a run
    /// of the workspace (a goal's run's files are its goal's). A workstream is
    /// its **checkout**, not the `<id>.json` record beside it: the record is
    /// bookkeeping, and the checkout is the thing somebody wants to look at.
    /// For the primary that is the project's *resolved* root, which for an
    /// adopted (`External`) project is outside the workspace entirely — so the
    /// containment base for anything read under it is this path, not the
    /// workspace root. A work item is wherever it actually ran — see
    /// [`Workspace::work_item_root`].
    pub fn file_root(&self, scope: FileScope, id: &str) -> Result<PathBuf, StoreError> {
        match scope {
            FileScope::Goal => {
                let id: GoalId = id.parse().map_err(|_| {
                    StoreError::Invalid(bisa_core::text!(
                        "error-store-invalid-not-goal-id",
                        id = format!("{id:?}")
                    ))
                })?;
                // Existence, before a path is handed out: answering "here is
                // where it would be" for a typo is what `placement` is for,
                // and it says so explicitly.
                self.get_goal(id)?;
                Ok(self.paths().goal(id).dir().to_path_buf())
            }
            FileScope::Workstream => {
                let id: WorkstreamId = id.parse().map_err(|_| {
                    StoreError::Invalid(bisa_core::text!(
                        "error-store-invalid-not-workstream-id",
                        id = format!("{id:?}")
                    ))
                })?;
                let workstream = self.get_workstream(id)?;
                self.workstream_checkout(&workstream)
            }
            FileScope::WorkItem => {
                let id: WorkItemId = id.parse().map_err(|_| {
                    StoreError::Invalid(bisa_core::text!(
                        "error-store-invalid-not-work-item-id",
                        id = format!("{id:?}")
                    ))
                })?;
                self.work_item_root(id)
            }
            FileScope::Run => {
                let run: RunId = id.parse().map_err(|_| {
                    StoreError::Invalid(bisa_core::text!(
                        "error-store-invalid-not-run-id",
                        id = format!("{id:?}")
                    ))
                })?;
                let run = self.get_run(run)?;
                if let Some(goal) = run.scope.goal() {
                    return Err(StoreError::Invalid(bisa_core::text!(
                        "error-store-invalid-run-files-are-its-goal-s",
                        run = run.id.to_string(),
                        goal = goal.to_string()
                    )));
                }
                Ok(self.paths().home(&run.home()).dir().to_path_buf())
            }
        }
    }

    /// Where a scope's files live, and whether the directory is there.
    ///
    /// `exists: false` is a real answer rather than an error: a project whose
    /// folder has not been created yet, or a workstream whose checkout was torn
    /// down, still has a place, and saying where it is beats a 404 that leaves
    /// the caller guessing which of the two things is missing.
    pub fn placement(&self, scope: FileScope, id: &str) -> Result<Placement, StoreError> {
        let path = self.file_root(scope, id)?;
        Ok(Placement {
            exists: path.exists(),
            path,
        })
    }

    /// List `relative` under a scope's root, bounded.
    ///
    /// `depth` is clamped into `1..=`[`TREE_MAX_DEPTH`] rather than refused: a
    /// caller asking for a hundred levels wants "everything", and the honest
    /// answer to that is the deepest listing this will do plus `truncated`.
    pub fn list_tree(
        &self,
        scope: FileScope,
        id: &str,
        relative: &str,
        depth: Option<usize>,
    ) -> Result<FileTree, StoreError> {
        let root = self.file_root(scope, id)?;
        if !root.is_dir() {
            return Err(StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-has-no-directory-yet",
                scope = scope.to_string(),
                id = id.to_string(),
                a0 = (root.display()).to_string()
            )));
        }
        // Both through the boundary, so `base` is canonical for the same
        // reason `start` is and the relative paths below strip cleanly. On
        // macOS the two spellings of a tempdir (`/var`, `/private/var`) would
        // otherwise never match.
        let base = resolve_within(&root, "")?;
        let start = resolve_within(&root, relative)?;
        let depth = depth.unwrap_or(TREE_DEFAULT_DEPTH).clamp(1, TREE_MAX_DEPTH);

        let layout = match scope {
            FileScope::Goal => {
                let goal = self.paths().goal(id.parse().map_err(|_| {
                    StoreError::Invalid(bisa_core::text!(
                        "error-store-invalid-not-goal-id",
                        id = format!("{id:?}")
                    ))
                })?);
                Some(Layout::of(&goal, Some(goal.documents())))
            }
            // A run of the workspace's folder is a home of the goal's shape,
            // with no documents: nobody gives a run its context as files.
            FileScope::Run => {
                let run: RunId = id.parse().map_err(|_| {
                    StoreError::Invalid(bisa_core::text!(
                        "error-store-invalid-not-run-id",
                        id = format!("{id:?}")
                    ))
                })?;
                Some(Layout::of(&self.paths().home(&Home::Run { run }), None))
            }
            // A workstream root is somebody's source tree; nothing in it is a
            // snapshot or a patch just because of where it sits. A work item
            // roots at one of those, or at its home's `scratch/` — which the
            // home layout already declines to classify, because what an agent
            // left in a working directory is an ordinary file.
            FileScope::Workstream | FileScope::WorkItem => None,
        };

        let (entries, bounds) = walk(&base, &start, depth, layout.as_ref());
        Ok(FileTree {
            root,
            path: relative.to_string(),
            depth,
            truncated: bounds.truncated,
            deeper: bounds.deeper,
            entries,
        })
    }

    /// Read one file under a scope's root, bounded and never as raw bytes for
    /// something that is not text.
    pub fn read_file(
        &self,
        scope: FileScope,
        id: &str,
        relative: &str,
    ) -> Result<FileContent, StoreError> {
        let root = self.file_root(scope, id)?;
        let path = resolve_within(&root, relative)?;
        read_bounded(&path, relative, ReadCap::INSPECTOR)
    }

    /// The same read, the same containment check and the same binary sniff,
    /// with the caller's cap — one implementation, two callers.
    pub fn read_file_capped(
        &self,
        scope: FileScope,
        id: &str,
        relative: &str,
        cap: ReadCap,
    ) -> Result<FileContent, StoreError> {
        let root = self.file_root(scope, id)?;
        let path = resolve_within(&root, relative)?;
        read_bounded(&path, relative, cap)
    }

    /// Where one file under a scope's root is, and how big: the same
    /// containment check as a read, no bytes read. For a caller that serves
    /// the file as it is — a renderer's bytes — rather than as text; the
    /// caller decides what size it will serve. A missing path and a directory
    /// are refused by name, like a read's.
    pub fn file_path(
        &self,
        scope: FileScope,
        id: &str,
        relative: &str,
    ) -> Result<(PathBuf, u64), StoreError> {
        let root = self.file_root(scope, id)?;
        located(&root, relative)
    }
}

/// One file under `root`, contained and present: its path and its size.
fn located(root: &Path, relative: &str) -> Result<(PathBuf, u64), StoreError> {
    let path = resolve_within(root, relative)?;
    let meta = std::fs::metadata(&path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-no-such-file",
                relative = format!("{relative:?}")
            ))
        } else {
            StoreError::io(path.display().to_string(), e)
        }
    })?;
    if meta.is_dir() {
        return Err(StoreError::Invalid(bisa_core::text!(
            "error-store-invalid-directory-list-instead",
            relative = format!("{relative:?}")
        )));
    }
    Ok((path, meta.len()))
}

// ---------------------------------------------------------------------------
// The layout, as the authority spells it
// ---------------------------------------------------------------------------

/// A home's layout in relative form — a goal's folder or a run of the
/// workspace's — derived from [`HomePaths`].
///
/// Nothing here is a string literal. `paths.rs` owns where things go.
struct Layout {
    journal: PathBuf,
    /// Where a home's sessions run. Its children are ordinary files.
    work: PathBuf,
    /// Directories that are indexes of addressable things: every child of one
    /// *is* one of them — a snapshot, a patch, a note — so the annotation
    /// carries one level down and stops. Below that is content.
    indexes: Vec<(PathBuf, EntryKind)>,
}

impl Layout {
    /// A home's layout; `documents` is a goal's folder of them — a run of
    /// the workspace has none.
    fn of(home: &HomePaths, documents: Option<PathBuf>) -> Self {
        let rel = |p: PathBuf| {
            p.strip_prefix(home.dir())
                .map(Path::to_path_buf)
                .unwrap_or(p)
        };
        // A goal's notes live in the notes repository, not its folder.
        let mut indexes = vec![
            (rel(home.state()), EntryKind::State),
            (rel(home.results()), EntryKind::Result),
        ];
        if let Some(documents) = documents {
            indexes.push((rel(documents), EntryKind::Document));
        }
        Self {
            journal: rel(home.journal()),
            work: rel(home.scratch()),
            indexes,
        }
    }

    fn classify(&self, rel: &Path, is_dir: bool) -> EntryKind {
        if rel == self.journal {
            return EntryKind::Journal;
        }
        if rel == self.work {
            return EntryKind::Work;
        }
        for (dir, kind) in &self.indexes {
            if rel == dir || rel.parent() == Some(dir.as_path()) {
                return *kind;
            }
        }
        fallback(is_dir)
    }
}

fn fallback(is_dir: bool) -> EntryKind {
    if is_dir {
        EntryKind::Dir
    } else {
        EntryKind::File
    }
}

// ---------------------------------------------------------------------------
// The walk
// ---------------------------------------------------------------------------

/// Breadth-first, bounded by [`TREE_MAX_DEPTH`] and [`TREE_MAX_ENTRIES`],
/// never following a symlink. The two bounds come back apart ([`Bounds`]):
/// the depth bound is what a lazy client asked for, the entry bound is a cut.
///
/// Breadth-first is the one deliberate difference from `watch_scan`, which
/// pops a stack. A scan looking for changes does not care what order it hits
/// things in; a listing that is about to be cut does — a truncated answer that
/// spent its budget on one deep branch and dropped the root's own children is
/// worse than no answer, because it looks complete.
fn walk(
    base: &Path,
    start: &Path,
    max_depth: usize,
    layout: Option<&Layout>,
) -> (Vec<FileEntry>, Bounds) {
    let rules = ignore_rules(base);
    let mut out: Vec<FileEntry> = Vec::new();
    let mut bounds = Bounds::default();
    let mut queue: VecDeque<(PathBuf, usize)> = VecDeque::new();
    queue.push_back((start.to_path_buf(), 0));

    'walk: while let Some((dir, depth)) = queue.pop_front() {
        let mut entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            // A directory that cannot be read is one missing branch, not a
            // failed request: a permission bit somewhere inside an adopted
            // repository must not cost the whole listing.
            Err(_) => continue,
        };
        if depth >= max_depth {
            // Cut here rather than at the parent, so an empty directory
            // sitting exactly at the bound is reported as complete.
            bounds.deeper |= entries.next().is_some();
            continue;
        }
        for entry in entries.flatten() {
            if out.len() >= TREE_MAX_ENTRIES {
                bounds.truncated = true;
                break 'walk;
            }
            // A repository's internals are not a file listing. `.git` alone
            // holds thousands of objects, so one of them spends the whole
            // TREE_MAX_ENTRIES budget and returns `truncated` with the actual
            // work hidden behind it. `bisa-iso` skips it in both its copy
            // and its diff for the same reason; this is the third reader that
            // wants the working tree rather than the plumbing.
            if entry.file_name() == ".git" {
                continue;
            }
            let path = entry.path();
            // symlink_metadata, so a symlink is a leaf: a cycle is impossible
            // however the tree is arranged, and a link out of the root is
            // named without being followed.
            let Ok(meta) = path.symlink_metadata() else {
                continue;
            };
            let Ok(rel) = path.strip_prefix(base) else {
                continue;
            };
            let symlink = meta.is_symlink();
            let is_dir = meta.is_dir();
            let ignored = rules.matched_path_or_any_parents(&path, is_dir).is_ignore();
            out.push(FileEntry {
                path: rel.to_string_lossy().into_owned(),
                name: entry.file_name().to_string_lossy().into_owned(),
                kind: match layout {
                    Some(l) => l.classify(rel, is_dir),
                    None => fallback(is_dir),
                },
                dir: is_dir,
                symlink,
                size: (!is_dir).then_some(meta.len()),
                modified: meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs()),
                ignored,
            });
            if is_dir && !symlink {
                queue.push_back((path, depth + 1));
            }
        }
    }

    // `read_dir` order is whatever the filesystem felt like, and a listing
    // that reorders itself between two calls is one nobody can page through
    // or diff.
    out.sort_by(|a, b| a.path.cmp(&b.path));
    (out, bounds)
}

// ---------------------------------------------------------------------------
// The read
// ---------------------------------------------------------------------------

/// Read at most [`FILE_MAX_BYTES`], and decide from the bytes — never from the
/// extension — whether there is text to serve.
fn read_bounded(path: &Path, shown: &str, cap: ReadCap) -> Result<FileContent, StoreError> {
    let meta = std::fs::metadata(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            StoreError::Invalid(bisa_core::text!(
                "error-store-invalid-no-such-file-2",
                shown = format!("{shown:?}")
            ))
        } else {
            StoreError::io(path.display().to_string(), e)
        }
    })?;
    if meta.is_dir() {
        return Err(StoreError::Invalid(bisa_core::text!(
            "error-store-invalid-directory-list-instead-2",
            shown = format!("{shown:?}")
        )));
    }

    let mut buf = Vec::new();
    // One byte past the cap is how we learn we hit it. The stat's length would
    // do for an ordinary file and lie for anything the kernel synthesizes,
    // which reports zero and then hands over a megabyte.
    std::fs::File::open(path)
        .map_err(|e| StoreError::io(path.display().to_string(), e))?
        .take(cap.0 + 1)
        .read_to_end(&mut buf)
        .map_err(|e| StoreError::io(path.display().to_string(), e))?;
    let mut truncated = buf.len() as u64 > cap.0;
    buf.truncate(cap.0 as usize);

    let binary = FileContent {
        path: shown.to_string(),
        size: meta.len(),
        binary: true,
        truncated,
        text: None,
        hash: None,
    };

    // A NUL byte is the one tell that costs nothing and is never wrong about
    // text. The scan is over what was read rather than the whole file, which
    // the cap has already made a prefix.
    if buf.contains(&0) {
        return Ok(binary);
    }
    let text = match std::str::from_utf8(&buf) {
        Ok(s) => s.to_owned(),
        // `error_len: None` means the bytes end mid-character, which is what
        // the size cap cutting through a multi-byte character looks like —
        // serve the whole characters and let `truncated` say the rest is gone.
        Err(e) if e.error_len().is_none() => {
            truncated = true;
            String::from_utf8_lossy(&buf[..e.valid_up_to()]).into_owned()
        }
        // A real invalid sequence. Extension-based detection would have called
        // this file text on the strength of its name.
        Err(_) => return Ok(binary),
    };

    let hash = if truncated {
        None
    } else {
        Some(content_hash(&buf))
    };
    Ok(FileContent {
        path: shown.to_string(),
        size: meta.len(),
        binary: false,
        truncated,
        text: Some(text),
        hash,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Paths;

    fn tree(dir: &Path, depth: usize) -> (Vec<FileEntry>, Bounds) {
        let base = dir.canonicalize().unwrap();
        walk(&base, &base, depth, None)
    }

    const COMPLETE: Bounds = Bounds {
        truncated: false,
        deeper: false,
    };
    const DEEPER: Bounds = Bounds {
        truncated: false,
        deeper: true,
    };

    fn names(entries: &[FileEntry]) -> Vec<&str> {
        entries.iter().map(|e| e.path.as_str()).collect()
    }

    /// A renderer's bytes come from a path the same containment check
    /// admits — never one that leaves the root — and the answer is the
    /// file's size, read of nothing.
    #[test]
    fn a_file_is_located_inside_its_root_or_refused_by_name() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("docs")).unwrap();
        std::fs::write(dir.path().join("docs/deck.pdf"), b"%PDF-1.7 ...").unwrap();
        let (path, size) = located(dir.path(), "docs/deck.pdf").unwrap();
        assert_eq!(size, 12);
        assert!(path.ends_with("docs/deck.pdf"));
        assert!(
            matches!(
                located(dir.path(), "../deck.pdf"),
                Err(StoreError::Invalid(_))
            ),
            "a path that leaves the root is refused"
        );
        assert!(
            matches!(located(dir.path(), "docs/none.pdf"), Err(StoreError::Invalid(m)) if m.to_string().contains("no such file"))
        );
        assert!(
            matches!(located(dir.path(), "docs"), Err(StoreError::Invalid(m)) if m.to_string().contains("directory"))
        );
    }

    #[test]
    fn a_listing_stops_at_the_depth_it_was_given() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("a/b/c")).unwrap();
        std::fs::write(dir.path().join("a/b/c/deep.txt"), b"x").unwrap();

        let (entries, bounds) = tree(dir.path(), 1);
        assert_eq!(names(&entries), ["a"]);
        assert_eq!(
            bounds, DEEPER,
            "there is more under a/, and the answer says so — as depth, not as a cut"
        );

        let (entries, bounds) = tree(dir.path(), 2);
        assert_eq!(names(&entries), ["a", "a/b"]);
        assert_eq!(bounds, DEEPER);

        let (entries, bounds) = tree(dir.path(), 4);
        assert_eq!(names(&entries), ["a", "a/b", "a/b/c", "a/b/c/deep.txt"]);
        assert_eq!(bounds, COMPLETE, "the whole tree fit");
    }

    /// An empty directory sitting exactly at the bound is a complete answer.
    /// Reporting it as truncated would send a client after children that do
    /// not exist.
    #[test]
    fn an_empty_directory_at_the_bound_is_not_truncated() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("empty")).unwrap();

        let (entries, bounds) = tree(dir.path(), 1);
        assert_eq!(names(&entries), ["empty"]);
        assert_eq!(bounds, COMPLETE);
    }

    /// A repository's plumbing is not part of its file listing.
    ///
    /// `.git` alone holds thousands of objects, so a listing that walked into
    /// one would spend the whole entry budget on it and come back `truncated`
    /// with the working tree hidden behind refs and packfiles. `bisa-iso`
    /// skips it in both its copy and its diff; this is the third reader that
    /// wants the tree rather than the plumbing.
    #[test]
    fn a_listing_does_not_walk_into_a_repository() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".git/objects/ab")).unwrap();
        std::fs::write(dir.path().join(".git/HEAD"), b"ref: refs/heads/main").unwrap();
        std::fs::write(dir.path().join(".git/objects/ab/cdef"), b"x").unwrap();
        std::fs::write(dir.path().join("index.html"), b"<h1>calc</h1>").unwrap();
        std::fs::create_dir(dir.path().join("src")).unwrap();

        let (entries, bounds) = tree(dir.path(), 4);
        assert_eq!(names(&entries), ["index.html", "src"]);
        assert_eq!(bounds, COMPLETE, "skipping .git is not a cut answer");
    }

    #[test]
    fn a_listing_stops_at_the_entry_count() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..TREE_MAX_ENTRIES + 50 {
            std::fs::write(dir.path().join(format!("f{i:05}")), b"x").unwrap();
        }
        let (entries, bounds) = tree(dir.path(), 1);
        assert_eq!(entries.len(), TREE_MAX_ENTRIES);
        assert!(bounds.truncated, "the cap is the one bound a client shows");
    }

    /// A depth-one listing of a folder with subfolders is the lazy explorer's
    /// everyday request; it must not read as a cut, or every folder wears a
    /// warning.
    #[test]
    fn a_shallow_listing_with_children_below_is_deeper_not_truncated() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src/lib")).unwrap();
        std::fs::write(dir.path().join("src/main.rs"), b"x").unwrap();
        std::fs::write(dir.path().join("README.md"), b"x").unwrap();

        let (entries, bounds) = tree(dir.path(), 1);
        assert_eq!(names(&entries), ["README.md", "src"]);
        assert_eq!(bounds, DEEPER);
    }

    /// The bound the walk cannot be talked out of: a link is a leaf, so a
    /// directory that contains a link to itself terminates instead of
    /// recursing until the stack or the entry count gives out.
    #[test]
    #[cfg(unix)]
    fn a_symlink_is_listed_and_never_followed() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret"), b"x").unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("out")).unwrap();
        std::os::unix::fs::symlink(dir.path(), dir.path().join("loop")).unwrap();

        let (entries, _) = tree(dir.path(), TREE_MAX_DEPTH);
        assert_eq!(names(&entries), ["loop", "out"]);
        assert!(entries.iter().all(|e| e.symlink));
        assert!(
            !entries.iter().any(|e| e.path.contains("secret")),
            "the link's target leaked into the listing"
        );
    }

    /// Position in the layout decides, and the layout comes from `paths.rs`.
    #[test]
    fn a_goal_entry_says_what_it_is() {
        let dir = tempfile::tempdir().unwrap();
        let id = GoalId::from_ulid(ulid::Ulid::from_datetime(std::time::SystemTime::now()));
        let goal = Paths::new(dir.path()).goal(id);
        let layout = Layout::of(&goal, Some(goal.documents()));

        let rel = |p: PathBuf| p.strip_prefix(goal.dir()).unwrap().to_path_buf();
        assert_eq!(
            layout.classify(&rel(goal.journal()), false),
            EntryKind::Journal
        );
        assert_eq!(layout.classify(&rel(goal.scratch()), true), EntryKind::Work);
        assert_eq!(layout.classify(&rel(goal.state()), true), EntryKind::State);
        assert_eq!(
            layout.classify(&rel(goal.results()), true),
            EntryKind::Result
        );
        assert_eq!(
            layout.classify(
                &rel(
                    goal.result(bisa_core::WorkItemId::from_ulid(ulid::Ulid::from_datetime(
                        std::time::SystemTime::now()
                    )))
                ),
                false
            ),
            EntryKind::Result,
            "a child of an index is one of the things it indexes"
        );
        assert_eq!(
            layout.classify(&rel(goal.dir().join("notes")), true),
            EntryKind::Dir,
            "a goal's folder holds no notes any more: they are the notes repository's"
        );
        assert_eq!(
            layout.classify(&rel(goal.scratch().join("notes.md")), false),
            EntryKind::File
        );
        assert_eq!(layout.classify(&rel(goal.ledger()), false), EntryKind::File);
        assert_eq!(
            layout.classify(&rel(goal.documents()), true),
            EntryKind::Document
        );
    }

    /// A run of the workspace's folder is a home of the goal's shape — the
    /// same words for the same positions — and has no documents to name.
    #[test]
    fn a_workspace_run_entry_says_what_it_is() {
        let dir = tempfile::tempdir().unwrap();
        let run =
            bisa_core::RunId::from_ulid(ulid::Ulid::from_datetime(std::time::SystemTime::now()));
        let home = Paths::new(dir.path()).home(&Home::Run { run });
        let layout = Layout::of(&home, None);

        let rel = |p: PathBuf| p.strip_prefix(home.dir()).unwrap().to_path_buf();
        assert_eq!(
            layout.classify(&rel(home.journal()), false),
            EntryKind::Journal
        );
        assert_eq!(layout.classify(&rel(home.scratch()), true), EntryKind::Work);
        assert_eq!(layout.classify(&rel(home.state()), true), EntryKind::State);
        assert_eq!(
            layout.classify(&rel(home.results()), true),
            EntryKind::Result
        );
        assert_eq!(
            layout.classify(&rel(home.dir().join("documents")), true),
            EntryKind::Dir,
            "nobody gives a run its context as files"
        );
    }

    #[test]
    fn a_binary_file_is_named_and_never_served() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("logo.png");
        std::fs::write(&path, [0x89, b'P', b'N', b'G', 0x00, 0x01, 0x02]).unwrap();

        let content = read_bounded(&path, "logo.png", ReadCap::INSPECTOR).unwrap();
        assert!(content.binary);
        assert_eq!(content.text, None);
        assert_eq!(content.size, 7);
    }

    /// No NUL anywhere, and still not text. Extension-based detection would
    /// have called this one text on the strength of its name.
    #[test]
    fn invalid_utf8_without_a_nul_is_still_binary() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.txt");
        std::fs::write(&path, [b'h', b'i', 0xC3, 0x28, b'!']).unwrap();

        let content = read_bounded(&path, "notes.txt", ReadCap::INSPECTOR).unwrap();
        assert!(content.binary, "0xC3 0x28 is not a UTF-8 sequence");
    }

    #[test]
    fn an_oversized_file_is_truncated_rather_than_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.log");
        let bytes = vec![b'a'; FILE_MAX_BYTES as usize + 4096];
        std::fs::write(&path, &bytes).unwrap();

        let content = read_bounded(&path, "big.log", ReadCap::INSPECTOR).unwrap();
        assert!(content.truncated);
        assert!(!content.binary);
        assert_eq!(content.size, bytes.len() as u64);
        assert_eq!(content.text.unwrap().len(), FILE_MAX_BYTES as usize);
    }

    /// The cap lands mid-character. The reply is whole characters plus
    /// `truncated`, never a replacement glyph the caller would have to guess
    /// was ours.
    #[test]
    fn a_cut_through_a_character_serves_whole_characters() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wide.txt");
        let mut text = "a".repeat(FILE_MAX_BYTES as usize - 1);
        text.push('é'); // two bytes, the second past the cap
        std::fs::write(&path, text.as_bytes()).unwrap();

        let content = read_bounded(&path, "wide.txt", ReadCap::INSPECTOR).unwrap();
        assert!(content.truncated);
        let served = content.text.unwrap();
        assert_eq!(served.len(), FILE_MAX_BYTES as usize - 1);
        assert!(!served.contains('\u{fffd}'));
    }

    #[test]
    fn a_small_text_file_comes_back_whole() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("readme.md");
        std::fs::write(&path, "# hello\n").unwrap();

        let content = read_bounded(&path, "readme.md", ReadCap::INSPECTOR).unwrap();
        assert!(!content.binary && !content.truncated);
        assert_eq!(content.text.as_deref(), Some("# hello\n"));
        assert_eq!(content.path, "readme.md", "the reply names what was asked");
    }

    #[test]
    fn a_directory_is_not_a_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("state")).unwrap();
        let err = read_bounded(&dir.path().join("state"), "state", ReadCap::INSPECTOR).unwrap_err();
        assert!(err.to_string().contains("is a directory"), "{err}");
    }

    #[test]
    fn file_scopes_round_trip_through_their_names() {
        for scope in FileScope::ALL.iter().copied() {
            assert_eq!(scope.as_str().parse::<FileScope>().unwrap(), scope);
        }
        assert!("agent".parse::<FileScope>().is_err());
    }

    /// The terminal column and the wire field are the same word, so a client
    /// reading one and a person reading the other are looking at one thing.
    #[test]
    fn an_entry_kind_reads_the_same_in_both_places() {
        for kind in [
            EntryKind::Note,
            EntryKind::Work,
            EntryKind::Result,
            EntryKind::Journal,
            EntryKind::State,
            EntryKind::Document,
            EntryKind::Dir,
            EntryKind::File,
        ] {
            assert_eq!(
                serde_json::to_value(kind).unwrap(),
                serde_json::json!(kind.as_str())
            );
        }
    }
}

#[cfg(test)]
mod ignore_tests {
    use super::*;

    #[test]
    fn an_ignored_entry_is_listed_and_marked_only_inside_a_repository() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join(".gitignore"), "target/\n*.log\n").unwrap();
        std::fs::create_dir_all(dir.path().join("target")).unwrap();
        std::fs::write(dir.path().join("build.log"), "x").unwrap();
        std::fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();
        let (entries, _) = walk(dir.path(), dir.path(), 1, None);
        let by_name = |n: &str| entries.iter().find(|e| e.name == n).unwrap();
        assert!(by_name("target").ignored && by_name("target").dir);
        assert!(by_name("build.log").ignored);
        assert!(!by_name("main.rs").ignored);
        assert!(!by_name(".gitignore").ignored);

        let plain = tempfile::tempdir().unwrap();
        std::fs::write(plain.path().join(".gitignore"), "*.log\n").unwrap();
        std::fs::write(plain.path().join("build.log"), "x").unwrap();
        let (entries, _) = walk(plain.path(), plain.path(), 1, None);
        assert!(
            entries.iter().all(|e| !e.ignored),
            "no repository, no ignore rules"
        );
    }
}
