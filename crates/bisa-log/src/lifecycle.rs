//! Run markers: a small file under `runs/` for as long as a process runs,
//! removed by its goodbye. A marker left behind by a process that is no
//! longer alive is a run that ended without one — a signal, an abort, a
//! stack overflow, an allocation failure, a power cut — and the next start
//! of the same family says so, as an `error` line and a crash report, then
//! takes the marker away. A marker of a process still alive is left alone:
//! two commands run side by side all the time.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::crash::{self, CrashKind, CrashReport};
use crate::files::{files_in, is_marker_name, runs_dir, Process, REPORT_SUFFIX};

/// What a running process leaves under `runs/`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Marker {
    pub process: Process,
    pub version: String,
    pub pid: u32,
    /// RFC 3339, UTC.
    pub started_at: String,
}

impl Marker {
    pub fn file_name(&self) -> String {
        format!("{}.{}.{}", self.process.prefix(), self.pid, REPORT_SUFFIX)
    }
}

/// Write this run's marker. Answers its path, for the goodbye to remove.
pub fn mark(root: &Path, marker: &Marker) -> std::io::Result<PathBuf> {
    let dir = runs_dir(root);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(marker.file_name());
    let body = serde_json::to_vec_pretty(marker)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(&path, body)?;
    Ok(path)
}

/// One run that ended without a goodbye, as the sweep found it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stale {
    pub marker: Marker,
    /// Where the report went.
    pub report: PathBuf,
}

/// Every marker of `process`'s family whose pid is no longer alive — never
/// this process's own, never another family's — becomes one `error` line
/// and one abrupt-end report, and is removed. A marker that cannot be read
/// is removed with a `warn`: it names nothing a report could say.
pub fn sweep(root: &Path, process: Process, now: SystemTime) -> Vec<Stale> {
    let dir = runs_dir(root);
    let own = std::process::id();
    let mut found = Vec::new();
    let markers = files_in(&dir, |name| {
        is_marker_name(name) && name.starts_with(&format!("{}.", process.prefix()))
    })
    .unwrap_or_default();
    for file in markers {
        let path = dir.join(&file.name);
        let marker: Marker = match std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
        {
            Some(m) => m,
            None => {
                tracing::warn!(
                    target: "bisa_log",
                    file = %path.display(), // LCOV_EXCL_LINE: a tracing line's fields are counted on the macro's own line; the line they make is read back by the crate's tests
                    "a run marker could not be read and was removed"
                );
                remove_marker(&path);
                continue;
            }
        };
        if marker.pid == own || alive(marker.pid) {
            continue;
        }
        let message = format!(
            "the previous {} run (pid {}, version {}, started {}) ended without a goodbye",
            marker.process.prefix(),
            marker.pid,
            marker.version,
            marker.started_at
        );
        let mut report = CrashReport::new(CrashKind::AbruptEnd, message.clone());
        report.process = marker.process;
        report.version = marker.version.clone();
        report.pid = marker.pid;
        report.at = crate::stamp::rfc3339(now);
        match crash::write(root, &report, now) {
            Ok(report_path) => {
                tracing::error!(
                    target: "bisa_log",
                    pid = marker.pid,
                    version = %marker.version,
                    started_at = %marker.started_at,
                    crash = %report_path.file_name().unwrap_or_default().to_string_lossy(), // LCOV_EXCL_LINE: a tracing line's fields are counted on the macro's own line; the line they make is read back by the crate's tests
                    "{message}"
                );
                remove_marker(&path);
                found.push(Stale {
                    marker,
                    report: report_path,
                });
            }
            Err(e) => {
                tracing::error!(
                    target: "bisa_log",
                    pid = marker.pid,
                    "{message}; no report could be written: {e}"
                );
                remove_marker(&path);
            }
        }
    }
    found
}

/// Whether a process with this pid exists. `kill(pid, 0)` sends nothing;
/// `EPERM` is a process of someone else's, alive. Where the question cannot
/// be asked, the answer is alive — a marker is then never reported, never
/// wrongly.
#[cfg(unix)]
pub fn alive(pid: u32) -> bool {
    let Ok(pid) = i32::try_from(pid) else {
        return false;
    };
    // SAFETY: `kill` with signal 0 checks the pid and delivers nothing.
    let rc = unsafe { libc::kill(pid, 0) };
    rc == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(not(unix))]
pub fn alive(_pid: u32) -> bool {
    true
}

/// Remove a marker whose run is over; one that will not go is said, never
/// fatal — the next sweep tries again.
fn remove_marker(path: &Path) {
    if let Err(error) = std::fs::remove_file(path) {
        // LCOV_EXCL_START: the marker was just read from this folder; a removal refused here is a permission the next sweep retries
        tracing::warn!(target: "bisa_log", file = %path.display(), %error, "a run marker could not be removed");
        // LCOV_EXCL_STOP
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::layer::SubscriberExt as _;
    use tracing_subscriber::util::SubscriberInitExt as _;

    fn marker(process: Process, pid: u32) -> Marker {
        Marker {
            process,
            version: "0.0.0-test".to_string(),
            pid,
            started_at: "2026-09-11T10:00:00Z".to_string(),
        }
    }

    /// A pid nobody has: the largest a `kill` accepts, which no system hands
    /// out.
    const DEAD: u32 = i32::MAX as u32 - 1;

    #[test]
    fn this_process_is_alive_and_a_pid_nobody_has_is_not() {
        assert!(alive(std::process::id()));
        assert!(!alive(DEAD));
    }

    #[test]
    fn a_stale_marker_of_the_family_becomes_a_report_and_the_live_one_stays() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let recording = crate::recorder::Recording::new();
        let _guard = tracing_subscriber::registry()
            .with(recording.layer())
            .set_default();
        let dead = mark(root, &marker(Process::Node, DEAD)).unwrap();
        let live = mark(root, &marker(Process::Node, std::process::id())).unwrap();
        let other = mark(root, &marker(Process::Cli, DEAD)).unwrap();
        std::fs::write(runs_dir(root).join("node.99.json"), "not json").unwrap();

        let found = sweep(root, Process::Node, SystemTime::now());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].marker.pid, DEAD);
        assert!(found[0].report.exists());
        assert!(!dead.exists(), "the stale marker went");
        assert!(live.exists(), "this process's marker stays");
        assert!(
            other.exists(),
            "another family's marker is not ours to judge"
        );
        assert!(
            !runs_dir(root).join("node.99.json").exists(),
            "an unreadable marker went"
        );

        let report = crash::read(
            root,
            &found[0].report.file_name().unwrap().to_string_lossy(),
        )
        .unwrap();
        assert_eq!(report.kind, CrashKind::AbruptEnd);
        assert_eq!(report.process, Process::Node);
        assert_eq!(report.pid, DEAD);
        assert!(report.message.contains("ended without a goodbye"));

        let said = recording.recorder.snapshot();
        assert!(said
            .iter()
            .any(|r| r.level == "ERROR" && r.message.contains("without a goodbye")));
        assert!(said
            .iter()
            .any(|r| r.level == "WARN" && r.message.contains("could not be read")));

        // Swept, nothing is found twice.
        assert!(sweep(root, Process::Node, SystemTime::now()).is_empty());
    }

    // added by the coverage pass: b6-lifecycle.rs
    #[test]
    fn a_pid_past_the_kill_range_is_nobody_and_a_report_that_cannot_be_written_still_removes_the_marker(
    ) {
        assert!(!alive(u32::MAX));
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let recording = crate::recorder::Recording::new();
        let _guard = tracing_subscriber::registry()
            .with(recording.layer())
            .set_default();
        let dead = mark(root, &marker(Process::Node, DEAD)).unwrap();
        // The crashes folder is a file: no report can be written there.
        std::fs::write(crate::files::crashes_dir(root), "in the way").unwrap();
        let found = sweep(root, Process::Node, SystemTime::now());
        assert!(found.is_empty(), "{found:?}");
        assert!(!dead.exists(), "the stale marker went all the same");
        assert!(recording
            .recorder
            .snapshot()
            .iter()
            .any(|r| r.level == "ERROR" && r.message.contains("no report could be written")));
    }
}
