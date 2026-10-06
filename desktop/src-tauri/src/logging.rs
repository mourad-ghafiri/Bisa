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

/// Ask the `bisa` binary where the workspace's log folder is and attach
/// the file layer there, at the default — errors only — until the webview
/// forwards the settings. The first candidate binary that answers wins;
/// answers the folder. No binary that answers is the error, with what each
/// said.
pub fn attach_from_binary(log: &Handle) -> Result<PathBuf, String> {
    let login_path = crate::login_env::login_path();
    let mut errors = Vec::new();
    for bin in crate::sidecar::candidate_binaries() {
        let mut cmd = Command::new(&bin);
        cmd.args(["paths", "--json"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if let Some(path) = &login_path {
            cmd.env("PATH", path);
        }
        let output = match cmd.output() {
            Ok(o) => o,
            Err(e) => {
                errors.push(format!("{}: {e}", bin.display()));
                continue;
            }
        };
        if !output.status.success() {
            errors.push(format!(
                "{}: paths answered {}",
                bin.display(),
                output.status
            ));
            continue;
        }
        let dir = match logs_dir_of(&String::from_utf8_lossy(&output.stdout)) {
            Ok(dir) => dir,
            Err(e) => {
                errors.push(format!("{}: {e}", bin.display()));
                continue;
            }
        };
        log.attach(&dir, LogConfig::default())
            .map_err(|e| e.to_string())?;
        return Ok(dir);
    }
    Err(format!(
        "no bisa binary named the workspace. Tried:\n{}",
        errors.join("\n")
    ))
}

/// The `logs_dir` of an `bisa paths --json` answer.
fn logs_dir_of(text: &str) -> Result<PathBuf, String> {
    let answer: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("the paths answer is not JSON: {e}"))?;
    answer["logs_dir"]
        .as_str()
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| "the paths answer names no logs_dir".to_string())
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
    fn the_paths_answer_names_the_log_folder_or_says_why_not() {
        assert_eq!(
            logs_dir_of(r#"{"data_dir":"/x/.bisa","logs_dir":"/x/.bisa/logs"}"#).unwrap(),
            PathBuf::from("/x/.bisa/logs")
        );
        assert!(logs_dir_of(r#"{"data_dir":"/x"}"#)
            .unwrap_err()
            .contains("names no logs_dir"));
        assert!(logs_dir_of(r#"{"logs_dir":""}"#)
            .unwrap_err()
            .contains("names no logs_dir"));
        assert!(logs_dir_of("not json").unwrap_err().contains("not JSON"));
    }
}
