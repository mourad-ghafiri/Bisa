//! `git worktree`-based isolation: near-instant, CoW-free, and the diff is
//! native `git diff` output.
//!
//! The git plumbing lives in `bisa-vcs`, so this file is only the part
//! that is about *isolation*: an ephemeral, detached checkout that leaves no
//! branch behind and can be torn down even after somebody deleted it.

use std::path::Path;

use bisa_vcs::git;

use crate::{BackendKind, IsoError, IsoResult, IsolationBackend, ProbeResult};

pub struct GitWorktreeBackend;

impl IsolationBackend for GitWorktreeBackend {
    fn kind(&self) -> BackendKind {
        BackendKind::GitWorktree
    }

    /// Host-level: is a `git` binary on PATH? Whether `lower` is actually a
    /// repository is a per-path concern checked at `start`.
    fn probe(&self) -> ProbeResult {
        let probe = git::probe();
        if probe.available {
            ProbeResult::available()
        } else {
            ProbeResult::unavailable(
                probe
                    .reason
                    .unwrap_or_else(|| "git is unavailable".to_string()),
            )
        }
    }

    fn start(&self, lower: &Path, merged: &Path) -> IsoResult<()> {
        if !git::is_repo(lower) {
            return Err(IsoError::unavailable(format!(
                "{} is not inside a git repository",
                lower.display()
            )));
        }
        if merged.exists() {
            return Err(IsoError::other(format!(
                "merged path already exists: {}",
                merged.display()
            )));
        }
        // Detached: the isolation is thrown away when the work item ends, so
        // it must not leave a branch in the user's repository behind it.
        // Persisting work on a branch is a *workstream*, not isolation.
        git::worktree_add_detached(lower, merged, None)?;
        Ok(())
    }

    fn stop(&self, merged: &Path) -> IsoResult<()> {
        // Resolve the main repository through the worktree's common git dir
        // first: stop() needs no memory of `lower`, and the fallback below
        // still needs a repository to prune once `merged` is gone.
        let common = git::common_dir(merged)?;
        if git::worktree_remove(merged, true).is_err() {
            // A worktree git has already lost track of still has to disappear.
            std::fs::remove_dir_all(merged).map_err(|e| IsoError::other(e.to_string()))?;
            // Housekeeping after the tree is already gone: a prune that fails
            // leaves one stale entry git forgets on its own next prune.
            let _pruned = git::worktree_prune(&common);
        }
        Ok(())
    }
}
