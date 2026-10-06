//! Writes under a writable root: compare-and-swap saves, create, move and
//! delete. The node parses and renders; every decision is here.
//!
//! **Writable roots** are a project tree (managed or adopted — a person
//! editing their own repository through the IDE is the person writing),
//! a workstream checkout, a goal's `work/` and a work item's
//! placement. The journal, `state/`, `identity/` and the index are not files
//! the IDE can reach: a goal's *read* root is its directory, and its *write*
//! root is `work/`.
//!
//! **Compare-and-swap, never last-write-wins.** A save carries the hash of
//! the text it read; a mismatch is refused with the current text attached, so
//! the client can merge without a second round trip. A save with no hash is a
//! create, and is refused if the path exists.
//!
//! **A delete is reversible by default.** `editor.delete.trash` (on) moves
//! the entry to the OS trash through [`TrashRemover`]; off, [`UnlinkRemover`]
//! removes the bytes. The caller resolves the setting and passes a
//! [`Disposal`]; this module never reads settings and never guesses. A trash
//! that refuses is an error, never a silent fall-through to unlinking.
//!
//! **A delete is one act.** Several entries — a folder's untracked files
//! from the git panel, an explorer selection, the files an Undo unmakes —
//! go through [`delete_entries`] as one batch: checked whole before anything
//! goes, then handed to the remover as one list, which the OS trash takes as
//! one move (one sound, one *Put Back*) where a call per entry would play
//! the sound once for each. [`delete_entry`] is the batch of one.

use crate::events::FileChangeKind;
use crate::{EngineError, EngineEvent, EnginePayload, Inner};
use bisa_core::ProjectId;
use bisa_store::{content_hash, resolve_within, FileScope};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// How a delete disposes of the bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Disposal {
    /// Into the OS trash, where a person can get it back.
    Trash,
    /// Gone.
    Unlink,
}

impl Disposal {
    /// `editor.delete.trash` as a disposal.
    pub fn from_setting(trash: bool) -> Self {
        if trash {
            Disposal::Trash
        } else {
            Disposal::Unlink
        }
    }

    pub(crate) fn remover(self) -> Box<dyn Remover> {
        match self {
            Disposal::Trash => Box::new(TrashRemover),
            Disposal::Unlink => Box::new(UnlinkRemover),
        }
    }
}

/// One entry a batch disposes of: where it is, and whether it is a folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Removal {
    pub path: PathBuf,
    pub is_dir: bool,
}

/// Where a batch halted: how many entries went before it, and what halted it
/// (boxed: an `Err` is kept small).
#[derive(Debug)]
pub struct Halt {
    pub went: usize,
    pub error: Box<EngineError>,
}

/// One way of making an entry go away.
pub trait Remover {
    fn remove(&self, path: &Path, is_dir: bool) -> Result<(), EngineError>;

    /// Several entries as **one act** for the person, in order, halting at
    /// the first failure — how many went before it is the halt's. The
    /// default removes one after another; a remover whose medium takes a
    /// list as one move overrides it.
    fn remove_all(&self, entries: &[Removal]) -> Result<(), Halt> {
        for (went, entry) in entries.iter().enumerate() {
            self.remove(&entry.path, entry.is_dir)
                .map_err(|error| Halt {
                    went,
                    error: Box::new(error),
                })?;
        }
        Ok(())
    }
}

/// The OS trash: files and directories alike, recoverable by the person.
pub struct TrashRemover;

impl Remover for TrashRemover {
    fn remove(&self, path: &Path, is_dir: bool) -> Result<(), EngineError> {
        self.remove_all(&[Removal {
            path: path.to_path_buf(),
            is_dir,
        }])
        .map_err(|halt| *halt.error)
    }

    /// One call to the OS for the whole list. On macOS the Finder takes it
    /// as one move — one sound, one *Put Back* — where a call per entry
    /// would play the sound once for each; and the OS refuses the list
    /// whole, so nothing went when it errs.
    fn remove_all(&self, entries: &[Removal]) -> Result<(), Halt> {
        if entries.is_empty() {
            return Ok(());
        }
        trash::delete_all(entries.iter().map(|e| &e.path)).map_err(|e| Halt {
            went: 0,
            error: Box::new(EngineError::Invalid(match entries {
                [one] => bisa_core::text!(
                    "error-engine-invalid-could-not-move-trash",
                    a0 = (one.path.display()).to_string(),
                    e = e.to_string()
                ),
                _ => bisa_core::text!(
                    "error-engine-invalid-could-not-move-trash-several",
                    n = entries.len().to_string(),
                    e = e.to_string()
                ),
            })),
        })
    }
}

/// The bytes go. What the confirmation said is what happens.
pub struct UnlinkRemover;

impl Remover for UnlinkRemover {
    fn remove(&self, path: &Path, is_dir: bool) -> Result<(), EngineError> {
        if is_dir {
            std::fs::remove_dir_all(path)?;
        } else {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }
}

/// What a delete hands back: the path, and how it went.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Deleted {
    pub path: String,
    pub disposal: Disposal,
}

/// One entry a batch is asked to delete: its path, and whether a folder's
/// delete was confirmed — what the confirmation the client showed records,
/// entry by entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToDelete {
    pub path: String,
    pub recursive: bool,
}

/// What a batch delete hands back: every asked path that went, in the order
/// asked, and where it halted when it did.
#[derive(Debug)]
pub struct Deletions {
    pub deleted: Vec<Deleted>,
    pub halted: Option<HaltedAt>,
}

/// The first asked path that did not go, and why.
#[derive(Debug)]
pub struct HaltedAt {
    pub path: String,
    pub error: EngineError,
}

/// A boolean setting, resolved for a project (or the workspace when `None`),
/// with `default` when the registry has no value. The one settings read the
/// IDE's file routes make.
pub fn setting_bool(inner: &Inner, project: Option<ProjectId>, key: &str, default: bool) -> bool {
    inner
        .ws
        .settings(project)
        .ok()
        .and_then(|rows| {
            rows.into_iter()
                .find(|r| r.key == key)
                .and_then(|r| r.value.as_bool())
        })
        .unwrap_or(default)
}

/// The editor's size policy (ide/03) as this machine set it: a file is
/// edited up to `editable` bytes, opened read-only up to `refuse`, and
/// refused above — `editor.large_file.editable_mib` and
/// `editor.large_file.refuse_mib`, read where a file is read and where it is
/// saved, so a change holds from the next request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorCaps {
    pub editable: u64,
    pub refuse: u64,
}

impl EditorCaps {
    /// The two bounds when nobody set them, and when what is set cannot be
    /// read.
    pub const UNSET: EditorCaps = EditorCaps {
        editable: 2 * MIB,
        refuse: 20 * MIB,
    };
    /// The most *Editable up to* may say: what the save route's body is
    /// sized from, held equal to the registry's own bound by a test.
    pub const MOST_EDITABLE: u64 = 8 * MIB;

    pub fn of(inner: &Inner) -> Self {
        let set = |key: &str| -> Option<u64> {
            inner
                .ws
                .setting(key, None)
                .ok()
                .and_then(|row| row.value.as_u64())
                .map(|mib| mib * MIB)
        };
        EditorCaps {
            editable: set("editor.large_file.editable_mib").unwrap_or(Self::UNSET.editable),
            refuse: set("editor.large_file.refuse_mib").unwrap_or(Self::UNSET.refuse),
        }
    }
}

const MIB: u64 = 1024 * 1024;

/// What a write hands back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    pub path: String,
    pub hash: String,
    pub created: bool,
}

/// What the explorer may create.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Dir,
}

/// The root a scope's writes are confined to — narrower than its read root
/// for a goal and for a run of the workspace, whose folders expose the
/// journal and the snapshots to a reader and never to a writer: a writer is
/// given their `scratch/`.
pub fn writable_root(
    inner: &Arc<Inner>,
    scope: FileScope,
    id: &str,
) -> Result<PathBuf, EngineError> {
    let ws = &inner.ws;
    match scope {
        FileScope::Goal => {
            let goal = id.parse().map_err(|_| {
                EngineError::Invalid(bisa_core::text!(
                    "error-engine-invalid-not-goal-id",
                    id = format!("{id:?}")
                ))
            })?;
            ws.get_goal(goal)?;
            Ok(ws.paths().goal(goal).scratch())
        }
        FileScope::Run => {
            let run = id.parse().map_err(|_| {
                EngineError::Invalid(bisa_core::text!(
                    "error-engine-invalid-not-run-id",
                    id = format!("{id:?}")
                ))
            })?;
            // The store's root refuses an unknown run and a goal's, whose
            // files are its goal's.
            ws.file_root(scope, id)?;
            Ok(ws.paths().home(&bisa_core::Home::Run { run }).scratch())
        }
        FileScope::Workstream | FileScope::WorkItem => Ok(ws.file_root(scope, id)?),
    }
}

/// The largest file served as bytes to a renderer (ide/03 §Rendered
/// documents): a PDF, an image, a recording, a sheet or a document the
/// desktop draws from the bytes themselves. Above it the node refuses with
/// the size, and the desktop offers to reveal the file instead.
pub const RAW_REFUSE_BYTES: u64 = 256 * 1024 * 1024;

/// One file under a scope's read root, as it is on disk: where, and how big.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawFile {
    pub path: PathBuf,
    pub size: u64,
}

/// The file a renderer's bytes come from — contained by the same check every
/// file route applies, never read here; whether its size is served is the
/// caller's decision against [`RAW_REFUSE_BYTES`]. A goal's root is its read
/// root, so a journal file may be rendered and never written.
pub fn raw_file(
    inner: &Arc<Inner>,
    scope: FileScope,
    id: &str,
    relative: &str,
) -> Result<RawFile, EngineError> {
    let (path, size) = inner.ws.file_path(scope, id, relative)?;
    Ok(RawFile { path, size })
}

/// The root-relative name of a path under the root — what goes on the wire
/// and into a `FileChanged` event. Both sides are read through the same
/// canonical form, so a root reached through a symlink (`/var` on macOS)
/// still strips; a path that is not under the root is an error, never the
/// workspace's absolute location handed to the webview.
fn shown(root: &Path, path: &Path) -> Result<String, EngineError> {
    let real_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let real_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let rel = real_path
        .strip_prefix(&real_root)
        .or_else(|_| path.strip_prefix(root))
        .or_else(|_| path.strip_prefix(&real_root))
        .map_err(|_| {
            EngineError::Invalid(bisa_core::text!("error-engine-invalid-path-not-under-root"))
        })?;
    Ok(rel.to_string_lossy().into_owned())
}

fn announce(
    inner: &Arc<Inner>,
    scope: FileScope,
    id: &str,
    path: &str,
    kind: FileChangeKind,
    from: Option<String>,
    dir: bool,
) {
    // The engine just changed the tree here: drop the root's cached path index
    // so the next quick-open walk sees the change at once.
    super::index::invalidate(scope.as_str(), id);
    inner.emit(EngineEvent::global(EnginePayload::FileChanged {
        scope: scope.as_str().to_string(),
        id: id.to_string(),
        path: path.to_string(),
        kind,
        from,
        ignored: false,
        dir,
    }));
}

/// Save `text` at `relative`, guarded by the hash of what was read.
///
/// `base_hash: None` creates and refuses an existing path; `Some` refuses
/// unless the file's current bytes hash to it. The write is atomic (temp +
/// rename), so a reader never sees half a save.
pub fn write_file(
    inner: &Arc<Inner>,
    scope: FileScope,
    id: &str,
    relative: &str,
    text: &str,
    base_hash: Option<&str>,
) -> Result<Written, EngineError> {
    write_bytes(inner, scope, id, relative, text.as_bytes(), base_hash)
}

/// [`write_file`] for bytes that need not be text — what an Undo of an
/// agent's change to a picture writes back (ide/20). The same guard, the
/// same atomic write, the same `FileChanged`.
pub fn write_bytes(
    inner: &Arc<Inner>,
    scope: FileScope,
    id: &str,
    relative: &str,
    bytes: &[u8],
    base_hash: Option<&str>,
) -> Result<Written, EngineError> {
    let root = writable_root(inner, scope, id)?;
    std::fs::create_dir_all(&root)?;
    let path = resolve_within(&root, relative)?;
    if path.is_dir() {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-directory",
            relative = format!("{relative:?}")
        )));
    }
    let existing = match std::fs::read(&path) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };
    match (base_hash, &existing) {
        (None, Some(_)) => {
            return Err(EngineError::Invalid(bisa_core::text!(
                "error-engine-invalid-already-exists-send-base-hash-what-you",
                relative = format!("{relative:?}")
            )))
        }
        (Some(base), Some(on_disk)) => {
            let current_hash = content_hash(on_disk);
            if current_hash != base {
                return Err(EngineError::FileConflict {
                    path: relative.to_string(),
                    current_hash,
                    current_text: String::from_utf8_lossy(on_disk).into_owned(),
                });
            }
        }
        (Some(_), None) => {
            return Err(EngineError::FileConflict {
                path: relative.to_string(),
                current_hash: String::new(),
                current_text: String::new(),
            })
        }
        (None, None) => {}
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    bisa_store::write_atomic(&path, bytes)?;
    let created = existing.is_none();
    let rel = shown(&root, &path)?;
    announce(
        inner,
        scope,
        id,
        &rel,
        if created {
            FileChangeKind::Created
        } else {
            FileChangeKind::Modified
        },
        None,
        false,
    );
    Ok(Written {
        path: rel,
        hash: content_hash(bytes),
        created,
    })
}

/// Create an empty file or a directory.
pub fn create_entry(
    inner: &Arc<Inner>,
    scope: FileScope,
    id: &str,
    relative: &str,
    kind: EntryKind,
) -> Result<String, EngineError> {
    let root = writable_root(inner, scope, id)?;
    std::fs::create_dir_all(&root)?;
    let path = resolve_within(&root, relative)?;
    if path.exists() {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-already-exists",
            relative = format!("{relative:?}")
        )));
    }
    match kind {
        EntryKind::Dir => std::fs::create_dir_all(&path)?,
        EntryKind::File => {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?;
        }
    }
    let rel = shown(&root, &path)?;
    announce(
        inner,
        scope,
        id,
        &rel,
        FileChangeKind::Created,
        None,
        matches!(kind, EntryKind::Dir),
    );
    Ok(rel)
}

/// Rename or move within the root. The target must not exist.
pub fn move_entry(
    inner: &Arc<Inner>,
    scope: FileScope,
    id: &str,
    from: &str,
    to: &str,
) -> Result<String, EngineError> {
    let root = writable_root(inner, scope, id)?;
    let source = resolve_within(&root, from)?;
    let target = resolve_within(&root, to)?;
    let meta = std::fs::symlink_metadata(&source).map_err(|_| {
        EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-no-such-path",
            from = format!("{from:?}")
        ))
    })?;
    if root.canonicalize().map(|r| r == source).unwrap_or(false) {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-root-itself-not-something-move"
        )));
    }
    // Every refusal before anything is made: a move that is refused must
    // leave no folder behind it, and a folder moved into itself would have
    // made its new parents inside the very tree it was about to move.
    if meta.is_dir() && target != source && target.starts_with(&source) {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-inside",
            to = format!("{to:?}"),
            from = format!("{from:?}")
        )));
    }
    // On a disk that folds case, `Readme.md` → `README.md` resolves to the
    // file itself: the name asked for differs from the one on disk, and that
    // is a rename like any other. Anything else that exists is in the way.
    let wanted = Path::new(to).file_name();
    let case_only = target == source && wanted.is_some_and(|w| Some(w) != source.file_name());
    let target = match wanted {
        Some(name) if case_only => source.with_file_name(name),
        _ => target,
    };
    if !case_only && target.exists() {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-already-exists-2",
            to = format!("{to:?}")
        )));
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let from_rel = shown(&root, &source)?;
    std::fs::rename(&source, &target)?;
    let rel = shown(&root, &target)?;
    announce(
        inner,
        scope,
        id,
        &rel,
        FileChangeKind::Renamed,
        Some(from_rel),
        meta.is_dir(),
    );
    Ok(rel)
}

/// Copy a file or a directory within the root, as a duplicate. The target
/// must not exist; a directory is copied entry by entry, a symlink is
/// recreated as a symlink and never followed — which is why the walk lives
/// beside `writable_root` rather than in a generic helper.
pub fn copy_entry(
    inner: &Arc<Inner>,
    scope: FileScope,
    id: &str,
    from: &str,
    to: &str,
) -> Result<String, EngineError> {
    let root = writable_root(inner, scope, id)?;
    let source = resolve_within(&root, from)?;
    let target = resolve_within(&root, to)?;
    let meta = std::fs::symlink_metadata(&source).map_err(|_| {
        EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-no-such-path",
            from = format!("{from:?}")
        ))
    })?;
    if target.exists() {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-already-exists-2",
            to = format!("{to:?}")
        )));
    }
    if meta.is_dir() && target.starts_with(&source) {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-inside",
            to = format!("{to:?}"),
            from = format!("{from:?}")
        )));
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    copy_tree(&source, &target, &meta)?;
    let rel = shown(&root, &target)?;
    announce(
        inner,
        scope,
        id,
        &rel,
        FileChangeKind::Created,
        None,
        meta.is_dir(),
    );
    Ok(rel)
}

fn copy_tree(src: &Path, dst: &Path, meta: &std::fs::Metadata) -> std::io::Result<()> {
    if meta.is_symlink() {
        let link = std::fs::read_link(src)?;
        #[cfg(unix)]
        std::os::unix::fs::symlink(link, dst)?;
        #[cfg(not(unix))]
        let _ = link;
        return Ok(());
    }
    if meta.is_dir() {
        std::fs::create_dir(dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            let child = entry.path();
            let child_meta = child.symlink_metadata()?;
            copy_tree(&child, &dst.join(entry.file_name()), &child_meta)?;
        }
        return Ok(());
    }
    std::fs::copy(src, dst)?;
    Ok(())
}

/// Remove a file, or a directory when `recursive` — the confirmation the
/// client showed is what `recursive` records — by the given [`Disposal`]. The
/// root itself is never removed. One entry of [`delete_entries`], answered
/// as before: the one row, or the error that halted it.
pub fn delete_entry(
    inner: &Arc<Inner>,
    scope: FileScope,
    id: &str,
    relative: &str,
    recursive: bool,
    disposal: Disposal,
) -> Result<Deleted, EngineError> {
    let asked = [ToDelete {
        path: relative.to_string(),
        recursive,
    }];
    let deletions = delete_entries(inner, scope, id, &asked, disposal)?;
    match (deletions.halted, deletions.deleted.into_iter().next()) {
        (Some(halted), _) => Err(halted.error),
        (None, Some(gone)) => Ok(gone),
        (None, None) => Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-no-such-path-2",
            relative = format!("{relative:?}")
        ))),
    }
}

/// An entry of a batch past its checks: where it is, how the root names it,
/// and what it is.
struct Checked {
    path: PathBuf,
    rel: String,
    is_dir: bool,
}

/// Remove several entries as **one act** — a folder's untracked files from
/// the git panel, an explorer selection, the files an Undo unmakes — by the
/// given [`Disposal`]. Every entry is checked before anything goes: the
/// first refusal — the root itself, a path that leaves it, one that is not
/// there, a folder whose delete was not confirmed — refuses the whole batch
/// untouched, as does a batch naming nothing. On the canonical paths an
/// entry named twice goes once, and one under a folder of the batch goes
/// with the folder; both are answered as asked. The remover takes the rest
/// in the order asked and halts at the first failure: the answer lists every
/// asked path that went and names the first that did not.
pub fn delete_entries(
    inner: &Arc<Inner>,
    scope: FileScope,
    id: &str,
    asked: &[ToDelete],
    disposal: Disposal,
) -> Result<Deletions, EngineError> {
    if asked.is_empty() {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-nothing-named-delete"
        )));
    }
    let root = writable_root(inner, scope, id)?;
    let real_root = root.canonicalize().ok();
    let root_itself = || {
        EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-root-itself-not-something-delete"
        ))
    };
    let mut checked: Vec<Checked> = Vec::with_capacity(asked.len());
    for entry in asked {
        let trimmed = entry.path.trim();
        if trimmed.is_empty() || trimmed == "." || trimmed == "./" {
            return Err(root_itself());
        }
        let path = resolve_within(&root, &entry.path)?;
        if real_root.as_ref().is_some_and(|r| *r == path) {
            return Err(root_itself());
        }
        let meta = std::fs::symlink_metadata(&path).map_err(|_| {
            EngineError::Invalid(bisa_core::text!(
                "error-engine-invalid-no-such-path-2",
                relative = format!("{:?}", entry.path)
            ))
        })?;
        if meta.is_dir() && !entry.recursive {
            let n = std::fs::read_dir(&path).map(|d| d.count()).unwrap_or(0);
            return Err(EngineError::Invalid(bisa_core::text!(
                "error-engine-invalid-directory-holding-entries-confirm-with-recursive-true",
                relative = format!("{:?}", entry.path),
                n = n.to_string()
            )));
        }
        let rel = shown(&root, &path)?;
        checked.push(Checked {
            path,
            rel,
            is_dir: meta.is_dir(),
        });
    }
    // Which removal each asked entry rides on: its own, or the outermost
    // folder of the batch that holds it — a folder goes whole, so what is
    // under it needs no move of its own, and a path named twice has one.
    let holds = |holder: &Checked, held: &Checked| {
        holder.path == held.path || (holder.is_dir && held.path.starts_with(&holder.path))
    };
    let rides: Vec<usize> = (0..checked.len())
        .map(|i| {
            (0..checked.len())
                .filter(|&j| holds(&checked[j], &checked[i]))
                .min_by_key(|&j| (checked[j].path.components().count(), j))
                .unwrap_or(i)
        })
        .collect();
    let mut order: Vec<usize> = Vec::new();
    for &top in &rides {
        if !order.contains(&top) {
            order.push(top);
        }
    }
    let removals: Vec<Removal> = order
        .iter()
        .map(|&j| Removal {
            path: checked[j].path.clone(),
            is_dir: checked[j].is_dir,
        })
        .collect();
    let (went, halt) = match disposal.remover().remove_all(&removals) {
        Ok(()) => (removals.len(), None),
        Err(Halt { went, error }) => (went, Some(*error)),
    };
    for &j in &order[..went] {
        announce(
            inner,
            scope,
            id,
            &checked[j].rel,
            FileChangeKind::Removed,
            None,
            false,
        );
    }
    // Each asked entry, by the removal it rode on: gone with it, or not.
    let gone = |top: usize| {
        order
            .iter()
            .position(|&j| j == top)
            .is_some_and(|at| at < went)
    };
    let mut deleted = Vec::with_capacity(asked.len());
    let mut first_left = None;
    for (i, &top) in rides.iter().enumerate() {
        if gone(top) {
            deleted.push(Deleted {
                path: checked[i].rel.clone(),
                disposal,
            });
        } else if first_left.is_none() {
            first_left = Some(checked[i].rel.clone());
        }
    }
    let halted = match (first_left, halt) {
        (Some(path), Some(error)) => Some(HaltedAt { path, error }),
        _ => None,
    };
    Ok(Deletions { deleted, halted })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// A remover that refuses one path and remembers what it was asked.
    struct Picky {
        refuse: PathBuf,
        asked: RefCell<Vec<PathBuf>>,
    }

    impl Remover for Picky {
        fn remove(&self, path: &Path, _is_dir: bool) -> Result<(), EngineError> {
            self.asked.borrow_mut().push(path.to_path_buf());
            if path == self.refuse {
                Err(EngineError::Invalid(bisa_core::text!(
                    "error-engine-invalid-no-such-path-2",
                    relative = format!("{:?}", path.display())
                )))
            } else {
                Ok(())
            }
        }
    }

    fn removals(paths: &[&str]) -> Vec<Removal> {
        paths
            .iter()
            .map(|p| Removal {
                path: PathBuf::from(p),
                is_dir: false,
            })
            .collect()
    }

    #[test]
    fn the_default_batch_halts_at_the_first_failure_and_says_how_many_went() {
        let picky = Picky {
            refuse: PathBuf::from("/b"),
            asked: RefCell::new(Vec::new()),
        };
        let halt = picky
            .remove_all(&removals(&["/a", "/b", "/c"]))
            .unwrap_err();
        assert_eq!(halt.went, 1, "one went before the refusal");
        assert_eq!(
            *picky.asked.borrow(),
            vec![PathBuf::from("/a"), PathBuf::from("/b")],
            "nothing after the failure is tried"
        );
        assert!(picky.remove_all(&removals(&["/a", "/c"])).is_ok());
        assert!(picky.remove_all(&[]).is_ok(), "nothing asked, nothing went");
    }
}
