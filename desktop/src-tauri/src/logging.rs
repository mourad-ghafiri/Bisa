//! The desktop's diagnostic log (`bisa-log`): the shell's own lines and
//! the webview's, one file family — `desktop/desktop.<date>.jsonl` — under
//! the workspace's `logs/`, and the crash reports the shell writes about
//! the node it supervises.
//!
//! The shell installs the subscriber before anything else runs and attaches
//! the file in `setup` (`sidecar::boot`), **before the node exists** and never
//! before `build` — which a second launch of the app leaves through the
//! single-instance plugin (`second_launch.rs`), having written nothing: it asks the `bisa` binary where
//! the workspace is (`bisa paths --json` — the store's word, so the
//! shell never spells a workspace path itself,
//! `docs/reference/workspace-layout.md`). When the node answers it attaches
//! again from `GET /workspace`'s `logs_dir` if nothing is writing yet — the
//! first launch, before `~/.bisa` existed — and when no binary can be
//! found at all it falls back to the app's own log folder so its words about
//! a missing node land somewhere. The recorder's replay carries what was
//! said before any of that into the first file that opens.
//!
//! The webview reaches the file through one command, `log_event`, so it
//! never names a path; the machine's `logging.*` settings reach the file
//! layer through the other, `log_configure`, forwarded by the webview at
//! boot and on every settings change (ide/01: the file is this machine's,
//! the shell's to write).

use std::path::PathBuf;
use std::process::{Command, Stdio};

use bisa_log::{Handle, LogConfig, LogLevel, Process};
use serde::Deserialize;
use tauri::{Manager, State};

/// How much of a webview line the shell keeps, whatever the webview sent.
const MAX_MESSAGE: usize = 4096;
const MAX_FIELDS: usize = 8192;

/// The process's subscriber, once.
pub fn install() -> Handle {
    bisa_log::install(Process::Desktop, env!("CARGO_PKG_VERSION"))
}

/// What `bisa paths --json` answers: the workspace's folders, and who holds
/// its engine right now — a node, by pid and start — or nobody.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathsAnswer {
    pub data_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub engine_holder: Option<Holder>,
}

/// The process holding the workspace's engine lock, as the binary read it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Holder {
    pub pid: u32,
    /// Unix seconds at the holder's start.
    pub started_at: u64,
}

/// Ask the `bisa` binary where the workspace is and attach the file layer
/// under its log folder, at the default — errors only — until the webview
/// forwards the settings. The first candidate binary that answers wins;
/// answers what it said. No binary that answers is the error, with what
/// each said.
pub fn attach_from_binary(log: &Handle) -> Result<PathsAnswer, String> {
    let mut errors = Vec::new();
    for bin in crate::sidecar::candidate_binaries() {
        let answer = match ask_paths(&bin) {
            Ok(answer) => answer,
            Err(e) => {
                errors.push(format!("{}: {e}", bin.display()));
                continue;
            }
        };
        log.attach(&answer.logs_dir, LogConfig::default())
            .map_err(|e| e.to_string())?;
        return Ok(answer);
    }
    Err(format!(
        "no bisa binary named the workspace. Tried:\n{}",
        errors.join("\n")
    ))
}

/// One `bisa paths --json` with `bin`: the workspace's folders and the
/// engine's holder, without a node and without touching the workspace.
pub fn ask_paths(bin: &std::path::Path) -> Result<PathsAnswer, String> {
    let mut cmd = Command::new(bin);
    cmd.args(["paths", "--json"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if let Some(path) = crate::login_env::login_path() {
        cmd.env("PATH", path);
    }
    let output = cmd.output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(format!("paths answered {}", output.status));
    }
    paths_answer_of(&String::from_utf8_lossy(&output.stdout))
}

/// A `bisa paths --json` answer, read. An older binary says nothing of the
/// holder: nobody.
fn paths_answer_of(text: &str) -> Result<PathsAnswer, String> {
    let answer: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("the paths answer is not JSON: {e}"))?;
    let dir = |key: &str| {
        answer[key]
            .as_str()
            .filter(|d| !d.is_empty())
            .map(PathBuf::from)
            .ok_or_else(|| format!("the paths answer names no {key}"))
    };
    let logs_dir = dir("logs_dir")?;
    let data_dir = dir("data_dir")?;
    let engine_holder = match &answer["engine_holder"] {
        serde_json::Value::Null => None,
        holder => Some(
            serde_json::from_value::<Holder>(holder.clone())
                .map_err(|e| format!("the paths answer's engine_holder is not a holder: {e}"))?,
        ),
    };
    Ok(PathsAnswer {
        data_dir,
        logs_dir,
        engine_holder,
    })
}

/// Ask the node where this machine's log folder is and attach the file
/// layer there when nothing is writing yet, or when the node names another
/// folder than the one attached. Called when the node first answers and
/// after every restart, so a first launch — the workspace made by the node
/// itself — gets its file the moment there is a place for it.
pub fn attach_from_node(log: &Handle, api_base: &str, token: &str) -> Result<(), String> {
    let (host, port) = crate::terminal::authority(api_base)?;
    let raw = crate::terminal::get(&host, port, token, "/workspace")?;
    let (status, body) = crate::terminal::parse_http_response(&raw)?;
    if status != 200 {
        return Err(format!("the node answered {status} to /workspace"));
    }
    let answer: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| format!("the node's workspace answer is not JSON: {e}"))?;
    let dir = answer["logs_dir"]
        .as_str()
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| "the node's workspace answer names no logs_dir".to_string())?;
    if log.writing() && log.root().as_deref() == Some(dir.as_path()) {
        return Ok(());
    }
    log.attach(dir, log.config()).map_err(|e| e.to_string())
}

/// The last resort, once the app is built: with no binary to name the
/// workspace and no node to ask, the shell's own words — chiefly that there
/// is no node — go under the app's log folder, so a bundled app that cannot
/// find its node is never silent. The replay carries what was said so far.
pub fn attach_fallback<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let log = app.state::<Handle>();
    if log.writing() {
        return;
    }
    let dir = match app.path().app_log_dir() {
        Ok(dir) => dir,
        Err(e) => {
            tracing::warn!(target: "bisa_desktop", "no app log folder: {e}");
            return;
        }
    };
    if let Some(parent) = dir.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            tracing::warn!(target: "bisa_desktop", "no app log folder: {e}");
            return;
        }
    }
    match log.attach(&dir, log.config()) {
        Ok(()) => tracing::warn!(
            target: "bisa_desktop",
            dir = %dir.display(),
            "no workspace could be named; the desktop's log is under the app's own folder until the node answers"
        ),
        Err(e) => tracing::warn!(target: "bisa_desktop", "no log file: {e}"),
    }
}

/// The machine's `logging.*` settings, as the webview read them.
#[tauri::command]
pub fn log_configure(log: State<'_, Handle>, config: LogConfig) -> Result<(), String> {
    log.apply(config).map_err(|e| e.to_string())
}

/// One line from the webview: a level word, the surface it came from, the
/// message and its fields — already bounded by the webview's shaper, and
/// bounded again here, since the shell trusts no length it did not set.
#[derive(Debug, Deserialize)]
pub struct WebviewEvent {
    pub level: String,
    pub target: String,
    pub message: String,
    #[serde(default)]
    pub fields: serde_json::Value,
}

#[tauri::command]
pub fn log_event(event: WebviewEvent) -> Result<(), String> {
    let level = level_of(&event.level)?;
    emit(level, &event);
    Ok(())
}

/// The level word, or why it is not one.
fn level_of(word: &str) -> Result<LogLevel, String> {
    word.parse::<LogLevel>().map_err(|e| e.to_string())
}

fn bounded(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut cut: String = text.chars().take(max).collect();
    cut.push('…');
    cut
}

fn emit(level: LogLevel, event: &WebviewEvent) {
    let message = bounded(&event.message, MAX_MESSAGE);
    let fields = match &event.fields {
        serde_json::Value::Null => String::new(),
        other => bounded(&other.to_string(), MAX_FIELDS),
    };
    let origin = bounded(&event.target, 64);
    macro_rules! line {
        ($macro:ident) => {
            tracing::$macro!(
                target: "bisa_desktop::webview",
                origin = %origin,
                fields = %fields,
                "{message}"
            )
        };
    }
    match level {
        LogLevel::Error => line!(error),
        LogLevel::Warn => line!(warn),
        LogLevel::Info => line!(info),
        LogLevel::Debug => line!(debug),
        LogLevel::Trace => line!(trace),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_level_word_is_the_crates_and_a_stranger_is_refused_by_name() {
        assert_eq!(level_of("error").unwrap(), LogLevel::Error);
        assert_eq!(level_of("trace").unwrap(), LogLevel::Trace);
        let refused = level_of("loud").unwrap_err();
        assert!(refused.contains("loud"), "{refused}");
        assert!(
            refused.contains("error, warn, info, debug, trace"),
            "{refused}"
        );
    }

    #[test]
    fn a_webview_event_reads_with_or_without_fields_and_is_bounded() {
        let bare: WebviewEvent =
            serde_json::from_str(r#"{"level":"warn","target":"api","message":"m"}"#).unwrap();
        assert!(bare.fields.is_null());
        let full: WebviewEvent = serde_json::from_str(
            r#"{"level":"error","target":"view","message":"crashed","fields":{"status":500}}"#,
        )
        .unwrap();
        assert_eq!(full.fields["status"], 500);
        let long = "x".repeat(MAX_MESSAGE + 10);
        let cut = bounded(&long, MAX_MESSAGE);
        assert_eq!(cut.chars().count(), MAX_MESSAGE + 1);
        assert!(cut.ends_with('…'));
        assert_eq!(bounded("short", MAX_MESSAGE), "short");
    }

    #[test]
    fn the_paths_answer_names_the_folders_and_the_holder_or_says_why_not() {
        let bare =
            paths_answer_of(r#"{"data_dir":"/x/.bisa","logs_dir":"/x/.bisa/logs"}"#).unwrap();
        assert_eq!(bare.logs_dir, PathBuf::from("/x/.bisa/logs"));
        assert_eq!(bare.data_dir, PathBuf::from("/x/.bisa"));
        assert_eq!(bare.engine_holder, None, "an older binary names no holder");
        let free = paths_answer_of(
            r#"{"data_dir":"/x/.bisa","logs_dir":"/x/.bisa/logs","engine_holder":null}"#,
        )
        .unwrap();
        assert_eq!(free.engine_holder, None);
        let held = paths_answer_of(
            r#"{"data_dir":"/x/.bisa","logs_dir":"/x/.bisa/logs","engine_holder":{"pid":4242,"started_at":1700000000}}"#,
        )
        .unwrap();
        assert_eq!(
            held.engine_holder,
            Some(Holder {
                pid: 4242,
                started_at: 1_700_000_000
            })
        );
        assert!(paths_answer_of(r#"{"data_dir":"/x"}"#)
            .unwrap_err()
            .contains("names no logs_dir"));
        assert!(paths_answer_of(r#"{"logs_dir":"/x/logs"}"#)
            .unwrap_err()
            .contains("names no data_dir"));
        assert!(paths_answer_of(r#"{"logs_dir":"","data_dir":"/x"}"#)
            .unwrap_err()
            .contains("names no logs_dir"));
        assert!(paths_answer_of("not json")
            .unwrap_err()
            .contains("not JSON"));
        assert!(paths_answer_of(
            r#"{"data_dir":"/x","logs_dir":"/x/logs","engine_holder":{"pid":"four"}}"#
        )
        .unwrap_err()
        .contains("not a holder"));
    }
}
