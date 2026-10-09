//! One engine per workspace.
//!
//! Two engines on one workspace would both claim work items, both run the
//! signal worker and both write snapshots — and the CLI's embedded engine
//! used to start whenever a `/health` probe did not answer within 500 ms,
//! which a daemon busy with a long operation routinely fails. So the engine
//! takes an exclusive `flock` on `run/engine.lock` before it binds anything,
//! writes the holder's PID and start time into the file for the refusal
//! message, and keeps the descriptor open for its lifetime. A dead holder
//! releases the lock with its process, so there is nothing to reclaim by
//! hand and no stale-lock heuristic to get wrong.

use crate::EngineError;
use bisa_store::Paths;
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

/// Who holds the lock, as written into the lock file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockHolder {
    pub pid: u32,
    /// Unix seconds when that engine started.
    pub started_at: u64,
}

impl std::fmt::Display for LockHolder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "pid {} (started at {})", self.pid, self.started_at)
    }
}

/// The held lock. Dropping it releases the workspace: the `flock` lives
/// exactly as long as the open file handle, which is why the handle is kept
/// and never read again.
#[derive(Debug)]
pub struct EngineLock {
    _file: File,
    path: PathBuf,
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn open_lock_file(path: &PathBuf) -> std::io::Result<File> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    } // LCOV_EXCL_LINE: the lock file is under the workspace's `run/`, which always has a parent
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

fn try_flock(file: &File) -> Result<bool, std::io::Error> {
    match rustix::fs::flock(file, rustix::fs::FlockOperation::NonBlockingLockExclusive) {
        Ok(()) => Ok(true),
        Err(rustix::io::Errno::WOULDBLOCK) => Ok(false),
        Err(e) => Err(std::io::Error::from(e)), // LCOV_EXCL_LINE: `flock` on an open regular file fails only when held, which is `WOULDBLOCK`
    }
}

fn read_holder(file: &mut File) -> Option<LockHolder> {
    let mut text = String::new();
    file.seek(SeekFrom::Start(0)).ok()?;
    file.read_to_string(&mut text).ok()?;
    serde_json::from_str(text.trim()).ok()
}

impl EngineLock {
    /// Take the workspace's engine lock, or say who holds it.
    pub fn acquire(paths: &Paths) -> Result<Self, EngineError> {
        let path = paths.engine_lock();
        let mut file = open_lock_file(&path)?;
        if !try_flock(&file)? {
            let holder = read_holder(&mut file);
            return Err(EngineError::Locked {
                path: path.display().to_string(),
                holder,
            });
        }
        let holder = LockHolder {
            pid: std::process::id(),
            started_at: now_secs(),
        };
        file.set_len(0)?;
        file.seek(SeekFrom::Start(0))?;
        let record = serde_json::to_string(&holder).map_err(std::io::Error::other)?;
        file.write_all(record.as_bytes())?;
        file.flush()?;
        Ok(Self { _file: file, path })
    }

    /// Who holds the workspace right now, without taking it. `None` means
    /// nobody does — an engine may start.
    pub fn holder(paths: &Paths) -> Result<Option<LockHolder>, EngineError> {
        let path = paths.engine_lock();
        if !path.exists() {
            return Ok(None);
        }
        let mut file = open_lock_file(&path)?;
        if try_flock(&file)? {
            // We got it, so nobody had it. Release on drop — the file is not
            // rewritten, so a holder's record is never clobbered by a probe.
            return Ok(None);
        }
        Ok(Some(read_holder(&mut file).unwrap_or(LockHolder {
            pid: 0,
            started_at: 0,
        })))
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}
