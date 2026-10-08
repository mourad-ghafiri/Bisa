//! The folder: what sits under the workspace's `logs/`, and the read that
//! lists it for a panel or a bug report.
//!
//! ```text
//! logs/
//!   node/     node.<period>.jsonl           one folder per process family
//!   cli/      cli.<period>.jsonl
//!   mcp/      mcp.<period>.jsonl
//!   desktop/  desktop.<period>.jsonl
//!   crashes/  <process>.<stamp>.<pid>.json  one report per abnormal end
//!   runs/     <process>.<pid>.json          a run's marker, gone with its goodbye
//! ```
//!
//! The store names `logs/` itself (`bisa-store`'s `paths.rs`); this
//! crate names everything inside it. A stranger anywhere in here is never
//! listed and never removed.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

/// The extension every log file carries — JSON lines, like every other
/// line-per-record file of the workspace.
pub const SUFFIX: &str = "jsonl";

/// The extension a crash report and a run marker carry: one JSON document.
pub const REPORT_SUFFIX: &str = "json";

/// The folder the crash reports sit in, under the root.
pub const CRASHES: &str = "crashes";

/// The folder the run markers sit in, under the root.
pub const RUNS: &str = "runs";

/// Which process is writing: each kind has a folder and a file family of its
/// own (`node/node.<date>.jsonl`, `desktop/desktop.<date>.jsonl`, …) so two
/// processes never share a file and retention prunes one family at a time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Process {
    /// `bisa node` — the daemon.
    Node,
    /// A one-shot `bisa` command.
    Cli,
    /// `bisa mcp` and the hook personalities a session spawns.
    Mcp,
    /// The desktop shell and, through it, the webview.
    Desktop,
}

impl Process {
    pub const ALL: [Process; 4] = [Process::Node, Process::Cli, Process::Mcp, Process::Desktop];

    /// The folder's name and the file name's first segment.
    pub fn prefix(self) -> &'static str {
        match self {
            Process::Node => "node",
            Process::Cli => "cli",
            Process::Mcp => "mcp",
            Process::Desktop => "desktop",
        }
    }

    /// The process a prefix names, if any.
    pub fn from_prefix(prefix: &str) -> Option<Process> {
        Self::ALL.into_iter().find(|p| p.prefix() == prefix)
    }

    /// The family's folder under the root.
    pub fn dir(self, root: &Path) -> PathBuf {
        root.join(self.prefix())
    }
}

impl std::fmt::Display for Process {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.prefix())
    }
}

/// The crash reports' folder under the root.
pub fn crashes_dir(root: &Path) -> PathBuf {
    root.join(CRASHES)
}

/// The run markers' folder under the root.
pub fn runs_dir(root: &Path) -> PathBuf {
    root.join(RUNS)
}

/// One file, as a panel lists it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogFile {
    pub name: String,
    pub bytes: u64,
    /// When it was last written, unix seconds.
    pub modified_at: u64,
}

/// One process family: its folder and its files, newest first.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Family {
    pub process: Process,
    pub dir: PathBuf,
    pub files: Vec<LogFile>,
}

/// What the root holds: every family in [`Process::ALL`]'s order — a family
/// with no folder yet is there with no files — and the crash reports,
/// newest first.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Listing {
    pub families: Vec<Family>,
    pub crashes: Vec<LogFile>,
}

impl Listing {
    /// The size of every file and every report together.
    pub fn bytes(&self) -> u64 {
        self.families
            .iter()
            .flat_map(|f| f.files.iter())
            .chain(self.crashes.iter())
            .map(|f| f.bytes)
            .sum()
    }

    /// Every file of every family, newest first, for a flat read.
    pub fn files(&self) -> Vec<&LogFile> {
        let mut all: Vec<&LogFile> = self.families.iter().flat_map(|f| f.files.iter()).collect();
        all.sort_by(newest_first);
        all
    }
}

/// Whether a name is a log file of ours: `<process>.<period>.jsonl`.
pub fn is_log_name(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(&format!(".{SUFFIX}")) else {
        return false;
    };
    let Some((prefix, period)) = stem.split_once('.') else {
        return false;
    };
    !period.is_empty() && Process::from_prefix(prefix).is_some()
}

/// Whether a name is a crash report of ours: `<process>.<stamp>.<pid>.json`,
/// the stamp a UTC `YYYYMMDDTHHMMSSZ`, the pid digits. A route answers a
/// report by name, so the name is checked before any path is joined.
pub fn is_crash_name(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(&format!(".{REPORT_SUFFIX}")) else {
        return false;
    };
    let mut parts = stem.split('.');
    let (Some(prefix), Some(stamp), Some(pid), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    Process::from_prefix(prefix).is_some() && is_stamp(stamp) && is_digits(pid)
}

/// Whether a name is a run marker of ours: `<process>.<pid>.json`.
pub fn is_marker_name(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(&format!(".{REPORT_SUFFIX}")) else {
        return false;
    };
    let Some((prefix, pid)) = stem.split_once('.') else {
        return false;
    };
    Process::from_prefix(prefix).is_some() && is_digits(pid)
}

fn is_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// `YYYYMMDDTHHMMSSZ`: fifteen characters, digits around a `T` and a `Z`.
fn is_stamp(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 16
        && b[8] == b'T'
        && b[15] == b'Z'
        && b[..8].iter().all(u8::is_ascii_digit)
        && b[9..15].iter().all(u8::is_ascii_digit)
}

/// Every file of the root, grouped: the families and the crash reports. A
/// folder that does not exist yet is an empty list, not an error: nothing
/// has been written.
pub fn list(root: &Path) -> std::io::Result<Listing> {
    let mut families = Vec::with_capacity(Process::ALL.len());
    for process in Process::ALL {
        let dir = process.dir(root);
        let files = files_in(&dir, |name| {
            is_log_name(name) && name.starts_with(&format!("{}.", process.prefix()))
        })?;
        families.push(Family {
            process,
            dir,
            files,
        });
    }
    let crashes = files_in(&crashes_dir(root), is_crash_name)?;
    Ok(Listing { families, crashes })
}

/// The files of one folder that pass `keep`, newest first.
pub(crate) fn files_in(dir: &Path, keep: impl Fn(&str) -> bool) -> std::io::Result<Vec<LogFile>> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut files = Vec::new();
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !keep(&name) {
            continue;
        }
        let meta = entry.metadata()?;
        if !meta.is_file() {
            continue;
        }
        let modified_at = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        files.push(LogFile {
            name,
            bytes: meta.len(),
            modified_at,
        });
    }
    files.sort_by(|a, b| newest_first(&a, &b));
    Ok(files)
}

fn newest_first(a: &&LogFile, b: &&LogFile) -> std::cmp::Ordering {
    b.modified_at
        .cmp(&a.modified_at)
        .then_with(|| b.name.cmp(&a.name))
}

/// The path a family's file would have — for a message that names it.
pub(crate) fn family_glob(root: &Path, process: Process) -> PathBuf {
    process
        .dir(root)
        .join(format!("{}.<period>.{SUFFIX}", process.prefix()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_log_name_is_a_process_a_period_and_the_suffix() {
        assert!(is_log_name("node.2026-09-08.jsonl"));
        assert!(is_log_name("desktop.2026-09-08-14.jsonl"));
        assert!(!is_log_name("node.jsonl"));
        assert!(!is_log_name("node.2026-09-08.log"));
        assert!(!is_log_name("stranger.2026-09-08.jsonl"));
        assert!(!is_log_name(".DS_Store"));
    }

    #[test]
    fn a_crash_name_is_a_process_a_stamp_a_pid_and_json() {
        assert!(is_crash_name("node.20260911T102233Z.12345.json"));
        assert!(is_crash_name("desktop.20260911T102233Z.1.json"));
        assert!(!is_crash_name("node.20260911T102233Z.json"));
        assert!(!is_crash_name("node.2026-09-11.12345.json"));
        assert!(!is_crash_name("node.20260911T102233Z.12345.jsonl"));
        assert!(!is_crash_name("stranger.20260911T102233Z.12345.json"));
        assert!(!is_crash_name("node.20260911T102233Z.12345.extra.json"));
        assert!(!is_crash_name("../node.20260911T102233Z.12345.json"));
    }

    #[test]
    fn a_marker_name_is_a_process_a_pid_and_json() {
        assert!(is_marker_name("node.12345.json"));
        assert!(is_marker_name("cli.7.json"));
        assert!(!is_marker_name("node.json"));
        assert!(!is_marker_name("node.abc.json"));
        assert!(!is_marker_name("node.12345.jsonl"));
        assert!(!is_marker_name("stranger.12345.json"));
    }

    #[test]
    fn the_folders_and_the_glob_hang_off_the_root() {
        let root = Path::new("/x/logs");
        assert_eq!(Process::Mcp.dir(root), PathBuf::from("/x/logs/mcp"));
        assert_eq!(crashes_dir(root), PathBuf::from("/x/logs/crashes"));
        assert_eq!(runs_dir(root), PathBuf::from("/x/logs/runs"));
        assert_eq!(
            family_glob(root, Process::Mcp),
            PathBuf::from("/x/logs/mcp/mcp.<period>.jsonl")
        );
        assert_eq!(Process::from_prefix("desktop"), Some(Process::Desktop));
        assert_eq!(Process::from_prefix("stranger"), None);
        assert_eq!(Process::Cli.to_string(), "cli");
    }

    #[test]
    fn a_listing_sums_and_flattens_newest_first() {
        let file = |name: &str, bytes: u64, at: u64| LogFile {
            name: name.to_string(),
            bytes,
            modified_at: at,
        };
        let listing = Listing {
            families: vec![
                Family {
                    process: Process::Node,
                    dir: PathBuf::from("/x/logs/node"),
                    files: vec![file("node.2026-09-08.jsonl", 10, 5)],
                },
                Family {
                    process: Process::Cli,
                    dir: PathBuf::from("/x/logs/cli"),
                    files: vec![file("cli.2026-09-09.jsonl", 20, 9)],
                },
            ],
            crashes: vec![file("node.20260911T102233Z.1.json", 30, 1)],
        };
        assert_eq!(listing.bytes(), 60);
        let names: Vec<&str> = listing.files().iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["cli.2026-09-09.jsonl", "node.2026-09-08.jsonl"]);
    }

    // added by the coverage pass: b6-files.rs
    #[test]
    fn a_folder_that_is_a_file_is_an_error_and_a_folder_named_like_a_log_is_not_a_file() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("node");
        std::fs::write(&file, "not a folder").unwrap();
        assert!(files_in(&file, |_| true).is_err());
        let dir = tmp.path().join("logs");
        std::fs::create_dir_all(dir.join("node.2026-09-08.jsonl")).unwrap();
        std::fs::write(dir.join("node.2026-09-09.jsonl"), "x\n").unwrap();
        let found = files_in(&dir, is_log_name).unwrap();
        assert_eq!(
            found.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
            ["node.2026-09-09.jsonl"]
        );
    }
}
