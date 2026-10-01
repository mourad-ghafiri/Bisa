//! Plain recursive copy: the universal floor. Always available; pays the
//! full copy cost, so it terminates every fallback chain rather than leading
//! one.

use std::fs;
use std::path::Path;

use crate::{BackendKind, IsoError, IsoResult, IsolationBackend, ProbeResult};

pub struct CopyBackend;

fn copy_tree(from: &Path, to: &Path) -> IsoResult<()> {
    fs::create_dir_all(to).map_err(|e| IsoError::other(format!("create {}: {e}", to.display())))?;
    let entries = fs::read_dir(from)
        .map_err(|e| IsoError::other(format!("read_dir {}: {e}", from.display())))?;
    for entry in entries {
        let entry = entry.map_err(|e| IsoError::other(e.to_string()))?;
        let name = entry.file_name();
        // The copy is not a git tree: its diff runs in plain mode against
        // lower, so replicating .git would only slow the copy down.
        if name == ".git" {
            continue;
        }
        let src = entry.path();
        let dst = to.join(&name);
        let meta = entry
            .metadata()
            .map_err(|e| IsoError::other(e.to_string()))?;
        if meta.is_dir() {
            copy_tree(&src, &dst)?;
        } else if meta.is_file() {
            fs::copy(&src, &dst)
                .map_err(|e| IsoError::other(format!("copy {}: {e}", src.display())))?;
        }
        // Symlinks and special files are skipped in v1.
    }
    Ok(())
}

impl IsolationBackend for CopyBackend {
    fn kind(&self) -> BackendKind {
        BackendKind::Copy
    }

    fn probe(&self) -> ProbeResult {
        ProbeResult::available()
    }

    fn start(&self, lower: &Path, merged: &Path) -> IsoResult<()> {
        if merged.exists() {
            return Err(IsoError::other(format!(
                "merged path already exists: {}",
                merged.display()
            )));
        }
        copy_tree(lower, merged)
    }

    fn stop(&self, merged: &Path) -> IsoResult<()> {
        if merged.exists() {
            fs::remove_dir_all(merged).map_err(|e| IsoError::other(e.to_string()))?;
        }
        Ok(())
    }
}
