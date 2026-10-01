//! One writer per checkout.
//!
//! git serialises its own writers with `index.lock`, and a second writer that
//! meets the lock does not wait — it fails, *Another git process seems to be
//! running*. Two of this process's own commands on one checkout — a commit
//! and a stage, a checkout and a fetch — must therefore never overlap. This
//! registry gives every checkout one slot, keyed by its canonical path, and
//! [`hold`] hands the caller a guard that owns the slot until it is dropped.
//! [`crate::git::Git`]'s `write*` runners take it around every invocation
//! that writes the index, the refs, the config or the tree; the reads never
//! do, because under `GIT_OPTIONAL_LOCKS=0` a read never takes the lock.
//!
//! Standard library only — a `Mutex<bool>` and a `Condvar` per slot — so the
//! crate stays dependency-free and synchronous, as its contract says. Two
//! spellings of one checkout (`a/../a`, a symlink) are one slot, because the
//! key is the canonical path; a path that does not exist yet (`init`, `clone`,
//! `worktree add`) is keyed under its canonical parent.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, OnceLock};

/// One checkout's slot: taken or free, and the bell rung when it frees.
#[derive(Default)]
struct Slot {
    busy: Mutex<bool>,
    freed: Condvar,
}

/// The slot held; dropping it frees the checkout for the next writer.
pub(crate) struct RepoGuard {
    slot: Arc<Slot>,
}

impl Drop for RepoGuard {
    fn drop(&mut self) {
        let mut busy = self.slot.busy.lock().unwrap_or_else(|e| e.into_inner());
        *busy = false;
        self.slot.freed.notify_one();
    }
}

fn registry() -> &'static Mutex<HashMap<PathBuf, Arc<Slot>>> {
    static SLOTS: OnceLock<Mutex<HashMap<PathBuf, Arc<Slot>>>> = OnceLock::new();
    SLOTS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The key a checkout is filed under: its canonical path when it exists,
/// else its canonical parent joined with its own name — so the slot a
/// `clone` takes is the one the clone's first commit will take.
pub(crate) fn key_of(path: &Path) -> PathBuf {
    if let Ok(real) = path.canonicalize() {
        return real;
    }
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) if !parent.as_os_str().is_empty() => key_of(parent).join(name),
        _ => normalise(path),
    }
}

/// A lexical normalisation for a path nothing on disk can resolve.
fn normalise(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Hold the checkout at `path` until the guard drops. Blocks while another
/// writer of this process holds it; the wait has no timeout, because a
/// write's own timeout (`Timeouts`) bounds how long a holder can keep it.
pub(crate) fn hold(path: &Path) -> RepoGuard {
    let slot = {
        let mut slots = registry().lock().unwrap_or_else(|e| e.into_inner());
        Arc::clone(slots.entry(key_of(path)).or_default())
    };
    let mut busy = slot.busy.lock().unwrap_or_else(|e| e.into_inner());
    while *busy {
        busy = slot.freed.wait(busy).unwrap_or_else(|e| e.into_inner());
    }
    *busy = true;
    drop(busy);
    RepoGuard { slot }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn a_second_writer_on_the_same_checkout_waits_for_the_first() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        let (tx, rx) = mpsc::channel::<&'static str>();
        let first = hold(&path);
        let second = {
            let (path, tx) = (path.clone(), tx.clone());
            thread::spawn(move || {
                let guard = hold(&path);
                tx.send("second holds").unwrap();
                drop(guard);
            })
        };
        assert!(
            rx.recv_timeout(Duration::from_millis(200)).is_err(),
            "the second writer must wait while the first holds"
        );
        tx.send("first drops").unwrap();
        drop(first);
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            "first drops"
        );
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            "second holds"
        );
        second.join().unwrap();
    }

    #[test]
    fn two_checkouts_are_two_slots_and_never_wait_on_each_other() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let held_a = hold(a.path());
        let (tx, rx) = mpsc::channel();
        let path_b = b.path().to_path_buf();
        thread::spawn(move || {
            let _held_b = hold(&path_b);
            tx.send(()).unwrap();
        });
        assert!(
            rx.recv_timeout(Duration::from_secs(5)).is_ok(),
            "another checkout is not waited for"
        );
        drop(held_a);
    }

    #[test]
    fn two_spellings_of_one_checkout_are_one_slot() {
        let dir = tempfile::tempdir().unwrap();
        let inner = dir.path().join("repo");
        std::fs::create_dir(&inner).unwrap();
        let dotted = dir.path().join("repo").join("..").join("repo");
        assert_eq!(key_of(&inner), key_of(&dotted));
        let unborn = dir.path().join("not-yet").join("clone");
        assert_eq!(
            key_of(&unborn),
            key_of(dir.path()).join("not-yet").join("clone"),
            "a path not on disk is keyed under its canonical parent"
        );
        assert_eq!(normalise(Path::new("/x/./y/../z")), PathBuf::from("/x/z"));
    }
}
