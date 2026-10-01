//! A snapshot of a checkout as a tree — what a review of an agent's changes
//! compares the working tree against (ide/20).
//!
//! The snapshot is staged through a **private index file** the caller owns,
//! named by `GIT_INDEX_FILE`: the person's own index is read once, to seed
//! it with its stat cache, and never written. `add -A` into that index
//! honours `.gitignore`, so a snapshot is the tracked files and the
//! untracked ones a person would see in Changes — never `target/`.
//!
//! The objects land in the repository's object store and are named by
//! nothing: they are read within the turn that made them, and git collects
//! them in its own time. No ref is written, no commit is made, and nothing
//! in the working tree moves.

use crate::exec::s;
use crate::git::Git;
use crate::{VcsError, VcsResult};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// A tree object's id, as `write-tree` printed it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TreeId(String);

impl TreeId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

const INDEX_ENV: &str = "GIT_INDEX_FILE";

impl Git {
    /// The checkout as a tree, staged through `index_file`. The first
    /// snapshot seeds the private index from the checkout's own, so only
    /// what moved since is hashed; a later one reuses it.
    pub fn snapshot_tree(&self, root: &Path, index_file: &Path) -> VcsResult<TreeId> {
        if let Some(dir) = index_file.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| VcsError::other(format!("create {}: {e}", dir.display())))?;
        }
        if !index_file.exists() {
            self.seed_index(root, index_file);
        }
        let env = [(OsStr::new(INDEX_ENV), index_file.as_os_str())];
        self.run_env(root, &[s("add"), s("-A"), s("--"), s(".")], &env)?;
        let out = self.run_env(root, &[s("write-tree")], &env)?;
        Ok(TreeId(String::from_utf8_lossy(&out).trim().to_string()))
    }

    /// Copy the checkout's index beside ours, for its stat cache. A
    /// repository with no index yet — no commit, nothing staged — starts
    /// from an empty one, which `add -A` fills.
    fn seed_index(&self, root: &Path, index_file: &Path) {
        let Ok(out) = self.run(
            Some(root),
            &[s("rev-parse"), s("--git-path"), s("index")],
            false,
        ) else {
            return;
        };
        let named = PathBuf::from(String::from_utf8_lossy(&out).trim());
        let own = if named.is_absolute() {
            named
        } else {
            root.join(named)
        };
        // A copy that fails costs one full hash, never the snapshot.
        let _copied = std::fs::copy(own, index_file);
    }

    /// Every path whose content differs between two snapshots — added,
    /// changed or gone — relative to the root. A rename is its two ends.
    pub fn changed_between(
        &self,
        root: &Path,
        from: &TreeId,
        to: &TreeId,
    ) -> VcsResult<Vec<PathBuf>> {
        if from == to {
            return Ok(Vec::new());
        }
        let raw = self.run(
            Some(root),
            &[
                s("diff-tree"),
                s("-r"),
                s("-z"),
                s("--name-only"),
                s("--no-renames"),
                s(from.as_str()),
                s(to.as_str()),
            ],
            false,
        )?;
        Ok(raw
            .split(|b| *b == 0)
            .filter(|t| !t.is_empty())
            .map(|t| PathBuf::from(String::from_utf8_lossy(t).into_owned()))
            .collect())
    }

    /// A file's bytes as a snapshot holds them; `None` when the snapshot
    /// has no such file.
    pub fn blob_at(&self, root: &Path, tree: &TreeId, path: &Path) -> VcsResult<Option<Vec<u8>>> {
        let spec = format!("{}:{}", tree.as_str(), path.to_string_lossy());
        let out = self.capture(Some(root), &[s("cat-file"), s("blob"), s(spec)], false)?;
        Ok(out.success.then_some(out.stdout))
    }
}

/// See [`Git::snapshot_tree`].
pub fn snapshot_tree(root: &Path, index_file: &Path) -> VcsResult<TreeId> {
    crate::git::shared().snapshot_tree(root, index_file)
}

/// See [`Git::changed_between`].
pub fn changed_between(root: &Path, from: &TreeId, to: &TreeId) -> VcsResult<Vec<PathBuf>> {
    crate::git::shared().changed_between(root, from, to)
}

/// See [`Git::blob_at`].
pub fn blob_at(root: &Path, tree: &TreeId, path: &Path) -> VcsResult<Option<Vec<u8>>> {
    crate::git::shared().blob_at(root, tree, path)
}
