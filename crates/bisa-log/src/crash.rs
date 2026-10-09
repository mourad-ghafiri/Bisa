//! A crash report: one JSON document per abnormal end under `crashes/`,
//! written at the moment of the panic — or at the next start, for a run
//! that ended without a goodbye — with everything a person needs to read
//! the death back: the words, the location, the thread, a backtrace, a
//! child's exit status and last stderr lines, and the flight recorder's
//! entries. The oldest past [`KEEP_CRASHES`] are removed when a new one is
//! written: with the rolled log files, the two things the platform removes
//! on its own.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::files::{crashes_dir, files_in, is_crash_name, Process, REPORT_SUFFIX};
use crate::recorder::Recorded;

/// How many reports the folder keeps, across every family.
pub const KEEP_CRASHES: usize = 50;

/// The most of a backtrace a report keeps.
const MAX_BACKTRACE: usize = 64 * 1024;

/// What ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrashKind {
    /// The process panicked; the hook wrote this before the unwind.
    Panic,
    /// A run of this family left its marker and never said goodbye — a
    /// signal, an abort, a stack overflow, an allocation failure, a power
    /// cut; written by the next start.
    AbruptEnd,
    /// A child this process supervises — the desktop's node — exited on
    /// its own.
    ChildExit,
}

impl CrashKind {
    pub const WORDS: [&'static str; 3] = ["panic", "abrupt_end", "child_exit"];

    pub fn as_str(self) -> &'static str {
        match self {
            CrashKind::Panic => "panic",
            CrashKind::AbruptEnd => "abrupt_end",
            CrashKind::ChildExit => "child_exit",
        }
    }
}

/// How a supervised child ended and what it last said.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChildExit {
    pub process: Process,
    pub pid: u32,
    /// The exit code, when it exited.
    pub code: Option<i32>,
    /// The signal that ended it, when one did.
    pub signal: Option<i32>,
    /// Its last stderr lines, oldest first.
    pub stderr: Vec<String>,
}

/// The report. [`CrashReport::new`] takes the kind and the words; the
/// handle stamps the process, the version, the pid, the moment and the
/// recorder's entries when it writes one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrashReport {
    pub process: Process,
    pub version: String,
    pub pid: u32,
    /// RFC 3339, UTC.
    pub at: String,
    pub kind: CrashKind,
    pub message: String,
    /// `file:line` of a panic.
    pub location: Option<String>,
    /// The thread that panicked, when it had a name.
    pub thread: Option<String>,
    pub backtrace: Option<String>,
    pub child: Option<ChildExit>,
    /// The flight recorder's entries at the moment, oldest first.
    pub recent: Vec<Recorded>,
}

impl CrashReport {
    /// A report with only what the site knows; the handle fills the rest.
    pub fn new(kind: CrashKind, message: impl Into<String>) -> Self {
        Self {
            process: Process::Cli,
            version: String::new(),
            pid: 0,
            at: String::new(),
            kind,
            message: message.into(),
            location: None,
            thread: None,
            backtrace: None,
            child: None,
            recent: Vec::new(),
        }
    }

    pub fn with_location(mut self, location: Option<String>) -> Self {
        self.location = location;
        self
    }

    pub fn with_thread(mut self, thread: Option<String>) -> Self {
        self.thread = thread;
        self
    }

    /// The backtrace, cut at [`MAX_BACKTRACE`] bytes on a line boundary.
    pub fn with_backtrace(mut self, backtrace: String) -> Self {
        self.backtrace = Some(if backtrace.len() <= MAX_BACKTRACE {
            backtrace
        } else {
            let mut cut = backtrace;
            let mut end = MAX_BACKTRACE;
            while !cut.is_char_boundary(end) {
                end -= 1;
            }
            cut.truncate(end);
            cut.push_str("\n…");
            cut
        });
        self
    }

    pub fn with_child(mut self, child: ChildExit) -> Self {
        self.child = Some(child);
        self
    }

    /// The file name a report of this process at this moment gets:
    /// `<process>.<stamp>.<pid>.json`.
    pub fn file_name(&self, at: SystemTime) -> String {
        format!(
            "{}.{}.{}.{}",
            self.process.prefix(),
            crate::stamp::name_stamp(at),
            self.pid,
            REPORT_SUFFIX
        )
    }
}

/// Write the report under `root/crashes/`, atomically — a temp file and a
/// rename, so a reader never sees half of one — and prune the oldest past
/// [`KEEP_CRASHES`]. Answers the path written.
pub fn write(root: &Path, report: &CrashReport, at: SystemTime) -> std::io::Result<PathBuf> {
    // A report never makes a workspace: the folder's parent must be there,
    // as `Handle::attach` asks before it opens a file.
    if !root.parent().map(Path::is_dir).unwrap_or(false) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "the log folder's parent does not exist",
        ));
    }
    let dir = crashes_dir(root);
    std::fs::create_dir_all(&dir)?;
    let name = report.file_name(at);
    let path = dir.join(&name);
    let temp = dir.join(format!(".{name}.tmp"));
    let body = serde_json::to_vec_pretty(report)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(&temp, body)?;
    std::fs::rename(&temp, &path)?;
    prune(&dir)?;
    Ok(path)
}

/// Remove the oldest reports past the count — every one of ours, by the
/// stamp in its name, so a stranger in the folder is never touched.
fn prune(dir: &Path) -> std::io::Result<()> {
    let mut ours: Vec<String> = files_in(dir, is_crash_name)?
        .into_iter()
        .map(|f| f.name)
        .collect();
    // The stamp sorts by time within a family and across them: sort by the
    // stamp alone, newest first, then remove from the tail.
    ours.sort_by(|a, b| stamp_of(b).cmp(stamp_of(a)));
    for name in ours.iter().skip(KEEP_CRASHES) {
        if let Err(error) = std::fs::remove_file(dir.join(name)) {
            // LCOV_EXCL_START: a report that will not go needs a folder this process may not write, which the write before the prune already refused
            tracing::warn!(target: "bisa_log", file = %name, %error, "an old crash report could not be pruned");
            // LCOV_EXCL_STOP
        }
    }
    Ok(())
}

fn stamp_of(name: &str) -> &str {
    name.split('.').nth(1).unwrap_or_default()
}

/// Read one report back by name — for the node's route and `bisa
/// logs`. A name that is not a crash name, or a file that is not there or
/// not a report, is `None`.
pub fn read(root: &Path, name: &str) -> Option<CrashReport> {
    if !is_crash_name(name) {
        return None;
    }
    let text = std::fs::read_to_string(crashes_dir(root).join(name)).ok()?;
    serde_json::from_str(&text).ok()
}

/// The newest report in the folder, with its file name, when there is one.
pub fn latest(root: &Path) -> Option<(String, CrashReport)> {
    let mut ours = files_in(&crashes_dir(root), is_crash_name).ok()?;
    ours.sort_by(|a, b| stamp_of(&b.name).cmp(stamp_of(&a.name)));
    let name = ours.into_iter().next()?.name;
    let report = read(root, &name)?;
    Some((name, report))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn report(kind: CrashKind, pid: u32) -> CrashReport {
        let mut r = CrashReport::new(kind, "it died");
        r.process = Process::Node;
        r.version = "0.0.0-test".to_string();
        r.pid = pid;
        r.at = "2026-09-11T10:22:33Z".to_string();
        r
    }

    #[test]
    fn a_report_is_written_read_back_by_name_and_pruned_past_the_count() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let at = SystemTime::UNIX_EPOCH + Duration::from_secs(1_789_000_000);
        let path = write(root, &report(CrashKind::Panic, 7), at).unwrap();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        assert_eq!(name, "node.20260910T002640Z.7.json");
        assert!(is_crash_name(&name));
        let back = read(root, &name).unwrap();
        assert_eq!(back.kind, CrashKind::Panic);
        assert_eq!(back.message, "it died");
        assert!(read(root, "../etc/passwd").is_none());
        assert!(read(root, "node.20260910T060640Z.8.json").is_none());
        let (latest_name, _) = latest(root).unwrap();
        assert_eq!(latest_name, name);

        // A stranger sits beside them and is never touched; past the count
        // the oldest of ours go.
        std::fs::write(crashes_dir(root).join("notes.json"), "{}").unwrap();
        for i in 0..(KEEP_CRASHES + 3) {
            let later = at + Duration::from_secs(60 * (i as u64 + 1));
            write(root, &report(CrashKind::AbruptEnd, 100 + i as u32), later).unwrap();
        }
        let left = files_in(&crashes_dir(root), is_crash_name).unwrap();
        assert_eq!(left.len(), KEEP_CRASHES);
        assert!(!crashes_dir(root).join(&name).exists(), "the oldest went");
        assert!(crashes_dir(root).join("notes.json").exists());
        let (latest_name, latest) = latest(root).unwrap();
        assert_eq!(latest.pid, 100 + KEEP_CRASHES as u32 + 2);
        assert!(latest_name.starts_with("node."));
    }

    #[test]
    fn a_backtrace_is_cut_on_a_char_boundary_and_the_kind_words_are_fixed() {
        let long = "é".repeat(MAX_BACKTRACE);
        let r = CrashReport::new(CrashKind::ChildExit, "x").with_backtrace(long);
        let bt = r.backtrace.unwrap();
        assert!(bt.ends_with("\n…"));
        assert!(bt.len() <= MAX_BACKTRACE + 4);
        assert_eq!(CrashKind::Panic.as_str(), "panic");
        assert_eq!(CrashKind::AbruptEnd.as_str(), "abrupt_end");
        assert_eq!(CrashKind::ChildExit.as_str(), "child_exit");
        assert_eq!(
            serde_json::to_string(&CrashKind::AbruptEnd).unwrap(),
            "\"abrupt_end\""
        );
    }

    // added by the coverage pass: b6-crash.rs
    #[test]
    fn a_backtrace_cut_inside_a_character_steps_back_to_its_boundary() {
        // One byte, then two-byte characters: the cap lands mid-character.
        let long = format!("a{}", "é".repeat(MAX_BACKTRACE));
        let report = CrashReport::new(CrashKind::Panic, "x").with_backtrace(long);
        let cut = report.backtrace.unwrap();
        assert!(cut.ends_with("\n…"));
        assert!(cut.len() < MAX_BACKTRACE + 8);
        assert!(cut.trim_end_matches("\n…").ends_with('é'));
    }
}
