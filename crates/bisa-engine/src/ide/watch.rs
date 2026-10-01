//! The filesystem watcher: the roots a person has open, and
//! nothing else.
//!
//! A client registers a root; `notify` watches it recursively; raw events are
//! coalesced per path over a 50 ms window and delivered on the engine bus as
//! `file_changed`. `.git/` internals are dropped except the closed list the
//! git panel shows — `HEAD`, `index`, `packed-refs`, `FETCH_HEAD`, the
//! operation markers (`ORIG_HEAD`, `MERGE_HEAD`, `REBASE_HEAD`,
//! `CHERRY_PICK_HEAD`, `REVERT_HEAD`) and everything under `refs/`,
//! `rebase-merge/`, `rebase-apply/` and `sequencer/`; `objects/` and `logs/`
//! stay dropped, because one commit writes dozens of objects and one `HEAD`
//! frame is the whole message. A root nobody has asked after for five
//! minutes is released; an OS-queue overflow is one `rescan` frame, and the
//! client refetches what it has open.
//!
//! A project start's `files` change keeps its polled look (the listening
//! ticker's): it runs unattended on any project a listener names; this runs
//! on roots a person is looking at.

use crate::events::FileChangeKind;
use crate::{EngineError, EngineEvent, EnginePayload, Inner};
use bisa_store::FileScope;
use notify::{RecursiveMode, Watcher as _};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long a root is watched with nobody asking after it.
pub const WATCH_IDLE: Duration = Duration::from_secs(300);
/// The coalescing window per path.
pub const DEBOUNCE: Duration = Duration::from_millis(50);

struct Entry {
    root: PathBuf,
    _watcher: notify::RecommendedWatcher,
    last_touch: Instant,
}

/// The root's ignore rules, the store's — the same rule the tree listing
/// marks entries with, so a frame and a row never disagree about `ignored`.
fn ignore_rules(root: &Path) -> ignore::gitignore::Gitignore {
    bisa_store::ignore_rules(root)
}

type Key = (String, String);

/// Every root being watched, keyed by `(scope, id)`.
#[derive(Default)]
pub struct WatchRegistry {
    entries: Mutex<HashMap<Key, Entry>>,
}

impl WatchRegistry {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<Key, Entry>> {
        self.entries.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// The roots currently watched.
    pub fn watched(&self) -> Vec<(String, String, PathBuf)> {
        self.lock()
            .iter()
            .map(|((s, i), e)| (s.clone(), i.clone(), e.root.clone()))
            .collect()
    }
}

/// Watch a scope's root, or refresh the lease on one already watched.
pub fn watch(inner: &Arc<Inner>, scope: FileScope, id: &str) -> Result<PathBuf, EngineError> {
    let root = inner.ws.file_root(scope, id)?;
    if !root.is_dir() {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-has-no-directory-yet-so-there-nothing-2",
            a0 = (scope.as_str()).to_string(),
            id = id.to_string()
        )));
    }
    // The OS reports canonical paths (`/private/var` for `/var` on macOS, a
    // project adopted through a symlink by its real location); the root is
    // kept the same way so every event strips to a relative path.
    let root = root.canonicalize()?;
    let key: Key = (scope.as_str().to_string(), id.to_string());
    {
        let mut entries = inner.ide_watch.lock();
        if let Some(e) = entries.get_mut(&key) {
            e.last_touch = Instant::now();
            return Ok(e.root.clone());
        }
    }
    let (tx, rx) = std::sync::mpsc::channel::<notify::Result<notify::Event>>();
    let mut watcher = notify::recommended_watcher(move |res| {
        // A closed channel means the collector is gone; nothing to do.
        if tx.send(res).is_err() {
            tracing::trace!("watcher event after its collector ended");
        }
    })
    .map_err(|e| {
        EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-watcher",
            e = e.to_string()
        ))
    })?;
    watcher
        .watch(&root, RecursiveMode::Recursive)
        .map_err(|e| {
            EngineError::Invalid(bisa_core::text!(
                "error-engine-invalid-watching",
                a0 = (root.display()).to_string(),
                e = e.to_string()
            ))
        })?;
    inner.ide_watch.lock().insert(
        key.clone(),
        Entry {
            root: root.clone(),
            _watcher: watcher,
            last_touch: Instant::now(),
        },
    );
    let collector_inner = Arc::clone(inner);
    let collector_root = root.clone();
    let rules = ignore_rules(&root);
    std::thread::Builder::new()
        .name(format!("ide-watch-{}", key.1))
        .spawn(move || collect(collector_inner, key, collector_root, rules, rx))
        .map_err(EngineError::Io)?;
    Ok(root)
}

/// Stop watching a root. Dropping the watcher closes the channel, which ends
/// the collector.
pub fn unwatch(inner: &Arc<Inner>, scope: FileScope, id: &str) -> bool {
    inner
        .ide_watch
        .lock()
        .remove(&(scope.as_str().to_string(), id.to_string()))
        .is_some()
}

/// Whether a path under `.git/` is one the git panel needs to hear about:
/// the working tree always; under `.git/`, the index, `HEAD`, the ref files
/// and directories, and the markers of an operation in progress — the
/// closed list the desktop's `gitChangeModel` reads its kinds from.
fn worth_reporting(rel: &Path) -> bool {
    let mut parts = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned());
    match parts.next().as_deref() {
        Some(".git") => matches!(
            parts.next().as_deref(),
            Some("HEAD")
                | Some("index")
                | Some("packed-refs")
                | Some("FETCH_HEAD")
                | Some("ORIG_HEAD")
                | Some("MERGE_HEAD")
                | Some("REBASE_HEAD")
                | Some("CHERRY_PICK_HEAD")
                | Some("REVERT_HEAD")
                | Some("refs")
                | Some("rebase-merge")
                | Some("rebase-apply")
                | Some("sequencer")
        ),
        _ => true,
    }
}

fn kind_of(kind: &notify::EventKind) -> Option<FileChangeKind> {
    use notify::EventKind::*;
    match kind {
        Create(_) => Some(FileChangeKind::Created),
        Modify(notify::event::ModifyKind::Name(_)) => Some(FileChangeKind::Renamed),
        Modify(_) => Some(FileChangeKind::Modified),
        Remove(_) => Some(FileChangeKind::Removed),
        Access(_) => None,
        Any | Other => Some(FileChangeKind::Modified),
    }
}

type Pending = BTreeMap<String, (FileChangeKind, Option<String>, bool)>;

/// The blocking collector: one per watched root. It ends when the root is
/// unwatched (the watcher drops and the channel closes) or the lease expires.
fn collect(
    inner: Arc<Inner>,
    key: Key,
    root: PathBuf,
    rules: ignore::gitignore::Gitignore,
    rx: std::sync::mpsc::Receiver<notify::Result<notify::Event>>,
) {
    let ignored = |rel: &str| {
        let full = root.join(rel);
        rules
            .matched_path_or_any_parents(&full, full.is_dir())
            .is_ignore()
    };
    let mut pending: Pending = BTreeMap::new();
    let mut last_event = Instant::now();
    loop {
        let wait = if pending.is_empty() {
            Duration::from_secs(1)
        } else {
            DEBOUNCE
        };
        match rx.recv_timeout(wait) {
            Ok(Ok(event)) => {
                last_event = Instant::now();
                if event.need_rescan() {
                    pending.insert(String::new(), (FileChangeKind::Rescan, None, false));
                    continue;
                }
                let Some(kind) = kind_of(&event.kind) else {
                    continue;
                };
                let mut rels: Vec<String> = event
                    .paths
                    .iter()
                    .filter_map(|p| match p.strip_prefix(&root) {
                        Ok(rel) => Some(rel),
                        Err(_) => {
                            // The root is canonical, so this is the watcher naming a
                            // path by another spelling: said, or a tree that reports
                            // nothing would have no line to explain it.
                            tracing::debug!(root = %root.display(), path = %p.display(), "a watched change is not under its root and is left out");
                            None
                        }
                    })
                    .filter(|rel| worth_reporting(rel))
                    .map(|rel| rel.to_string_lossy().into_owned())
                    .collect();
                if rels.is_empty() {
                    continue;
                }
                if kind == FileChangeKind::Renamed && rels.len() == 2 {
                    let from = rels.remove(0);
                    let to = rels.remove(0);
                    let ig = ignored(&to);
                    pending.insert(to, (FileChangeKind::Renamed, Some(from), ig));
                    continue;
                }
                for rel in rels {
                    // The later kind wins, except a create followed by a
                    // modify is still a create.
                    let ig = ignored(&rel);
                    let entry = pending.entry(rel).or_insert((kind, None, ig));
                    if !(entry.0 == FileChangeKind::Created && kind == FileChangeKind::Modified) {
                        *entry = (kind, None, ig);
                    }
                }
                continue;
            }
            Ok(Err(e)) => tracing::debug!("watcher error on {}: {e}", root.display()),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
        // The root itself went away under the watch — a checkout removed, a
        // volume unmounted. The OS says nothing more about it; the readers
        // are told to read again, and the watch ends rather than idling on
        // a directory that is gone.
        if !root.is_dir() {
            pending.insert(String::new(), (FileChangeKind::Rescan, None, false));
            flush(&inner, &key, &mut pending);
            inner.ide_watch.lock().remove(&key);
            return;
        }
        if !pending.is_empty() && last_event.elapsed() >= DEBOUNCE {
            flush(&inner, &key, &mut pending);
        }
        // The lease: nobody has asked after this root for a while.
        let expired = inner
            .ide_watch
            .lock()
            .get(&key)
            .map(|e| e.last_touch.elapsed() > WATCH_IDLE)
            .unwrap_or(true);
        if expired {
            inner.ide_watch.lock().remove(&key);
            break;
        }
    }
    flush(&inner, &key, &mut pending);
}

fn flush(inner: &Arc<Inner>, key: &Key, pending: &mut Pending) {
    // A change under this root makes its cached path index stale — and its
    // cached git status: what the panel reads next must be what the tree is,
    // whether the platform moved it or a terminal did.
    if !pending.is_empty() {
        super::index::invalidate(&key.0, &key.1);
        if key.0 == FileScope::Workstream.as_str() {
            if let Ok(id) = bisa_core::WorkstreamId::from_str(&key.1) {
                inner.ide_status.invalidate(id);
            }
        }
    }
    // Whether a path that is there is a folder is asked of the disk, once per
    // frame; one that is gone answers `false`.
    let root = FileScope::from_str(&key.0)
        .ok()
        .and_then(|scope| inner.ws.file_root(scope, &key.1).ok());
    for (path, (kind, from, ignored)) in std::mem::take(pending) {
        let dir = !path.is_empty() && root.as_ref().is_some_and(|r| r.join(&path).is_dir());
        inner.emit(EngineEvent::global(EnginePayload::FileChanged {
            scope: key.0.clone(),
            id: key.1.clone(),
            path,
            kind,
            from,
            ignored,
            dir,
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::worth_reporting;
    use std::path::Path;

    #[test]
    fn the_git_paths_the_panel_shows_are_reported_and_the_rest_are_not() {
        for rel in [
            ".git/HEAD",
            ".git/index",
            ".git/packed-refs",
            ".git/FETCH_HEAD",
            ".git/ORIG_HEAD",
            ".git/MERGE_HEAD",
            ".git/REBASE_HEAD",
            ".git/CHERRY_PICK_HEAD",
            ".git/REVERT_HEAD",
            ".git/refs/heads/main",
            ".git/refs/tags/v1",
            ".git/refs/remotes/origin/main",
            ".git/refs/stash",
            ".git/rebase-merge/done",
            ".git/rebase-apply/0001",
            ".git/sequencer/todo",
        ] {
            assert!(
                worth_reporting(Path::new(rel)),
                "{rel} drives the git panel"
            );
        }
        for rel in [
            ".git/objects/ab/cdef",
            ".git/logs/HEAD",
            ".git/logs/refs/stash",
            ".git/COMMIT_EDITMSG",
            ".git/config",
            ".git/hooks/pre-commit",
            ".git/info/exclude",
            ".git",
        ] {
            assert!(
                !worth_reporting(Path::new(rel)),
                "{rel} is nothing the panel shows"
            );
        }
    }

    #[test]
    fn a_working_tree_path_is_always_reported() {
        for rel in ["src/main.rs", "README.md", ".gitignore", "a/.git-like/file"] {
            assert!(worth_reporting(Path::new(rel)), "{rel}");
        }
    }
}
