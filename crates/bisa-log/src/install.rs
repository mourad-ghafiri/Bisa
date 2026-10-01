//! The subscriber, and the handle that retunes its file layer.
//!
//! [`install`] sets the process-wide subscriber once — stderr under
//! `RUST_LOG`, the flight recorder, the file behind two reload slots — and
//! the panic hook, and answers a [`Handle`]. [`Handle::attach`] gives the
//! file layer its folder and configuration; [`Handle::apply`] changes the
//! configuration in place. Both swap the slots and never the subscriber, so
//! every `tracing` call site keeps working and a settings change lands on
//! the next event.
//!
//! Two slots, not one: `reload::Handle::reload` refuses a `Filtered` layer
//! (tokio-rs/tracing#1629), so the level lives in a slot of its own above
//! the appender's — the appender is *reloaded* when the folder or the
//! rotation changes, the level is *modified*.
//!
//! The first file a process opens is the one moment it looks around: the
//! stale run markers of its family are swept into reports, its own marker
//! is written, and what the recorder heard before there was a file is
//! replayed into it.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Instant, SystemTime};

use tracing_appender::rolling::{InitError, RollingFileAppender};
use tracing_subscriber::filter::{Filtered, LevelFilter};
use tracing_subscriber::fmt::format::{Format, Json, JsonFields};
use tracing_subscriber::fmt::time::SystemTime as FmtTime;
use tracing_subscriber::fmt::MakeWriter as _;
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;
use tracing_subscriber::{fmt, reload, EnvFilter, Layer as _, Registry};

use crate::config::{LogConfig, LogLevel};
use crate::crash::{self, CrashKind, CrashReport};
use crate::files::{crashes_dir, family_glob, Process, SUFFIX};
use crate::lifecycle::{self, Marker};
use crate::recorder::{Recorded, Recorder, Recording, RecordingLayer, WrittenFlag, RECORDER_FLOOR};

/// The environment variable the engine sets on every child it spawns —
/// `bisa mcp`, the hook personalities — naming the folder the child
/// writes under. A child that is spawned without `--data-dir` would
/// otherwise resolve the default workspace, which may not be its parent's;
/// a child without the variable writes no file.
pub const LOG_DIR_ENV: &str = "BISA_LOG_DIR";

/// The file layer as it sits in its slot: JSON lines to the rolling appender.
type FileLayer = fmt::Layer<Registry, JsonFields, Format<Json, FmtTime>, RollingFileAppender>;
type FileReload = reload::Layer<Option<FileLayer>, Registry>;
type LevelReload = reload::Layer<LevelFilter, Registry>;

/// The file layer under its level filter.
pub type FileSlot = Filtered<FileReload, LevelReload, Registry>;
/// The flight recorder under its floor.
pub type RecorderSlot = Filtered<RecordingLayer, LevelFilter, Registry>;
/// What [`build`] hands back to be layered on a `Registry`, as one layer:
/// the file slot and the recorder. [`install`] does that on the global
/// subscriber; a test does it on a thread-local one.
pub type Layers = Vec<Box<dyn tracing_subscriber::Layer<Registry> + Send + Sync>>;

/// Attaching the file layer failed: the folder could not be made, the
/// appender could not open, or the slot refused the swap.
#[derive(Debug, thiserror::Error)]
pub enum AttachError {
    #[error("making the log folder: {0}")]
    Io(#[from] std::io::Error),
    #[error("opening the log file: {0}")]
    Appender(#[from] InitError),
    #[error("swapping the log layer: {0}")]
    Reload(#[from] reload::Error),
}

/// Writing a crash report failed: no folder was ever attached, the log is
/// switched off, or the write itself.
#[derive(Debug, thiserror::Error)]
pub enum ReportError {
    #[error("no log folder is attached")]
    NoFolder,
    #[error("the log is switched off")]
    Off,
    #[error("writing the report: {0}")]
    Io(#[from] std::io::Error),
}

struct State {
    root: Option<PathBuf>,
    config: LogConfig,
    /// The first open happened: swept, marked, replayed.
    opened: bool,
    /// A file layer is open right now.
    file_open: bool,
    /// This run's marker, for the goodbye to remove.
    marker: Option<PathBuf>,
    started: Instant,
    goodbye_said: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            root: None,
            config: LogConfig::default(),
            opened: false,
            file_open: false,
            marker: None,
            started: Instant::now(),
            goodbye_said: false,
        }
    }
}

struct Shared {
    process: Process,
    version: &'static str,
    file: reload::Handle<Option<FileLayer>, Registry>,
    level: reload::Handle<LevelFilter, Registry>,
    recorder: Recorder,
    written: WrittenFlag,
    state: Mutex<State>,
}

/// The one handle on a process's file layer. Cheap to clone; every clone
/// retunes the same layer.
#[derive(Clone)]
pub struct Handle {
    inner: Arc<Shared>,
}

impl std::fmt::Debug for Handle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = self.state();
        f.debug_struct("Handle")
            .field("process", &self.inner.process.prefix())
            .field("root", &state.root)
            .field("config", &state.config)
            .field("opened", &state.opened)
            .finish()
    }
}

/// The layers and their handle, not yet on any subscriber: the level is
/// `OFF` and the file slot empty until [`Handle::attach`]; the recorder
/// listens from the first event.
pub fn build(process: Process, version: &'static str) -> (Handle, Layers) {
    let (file_layer, file) = reload::Layer::new(None::<FileLayer>);
    let (level_layer, level) = reload::Layer::new(LevelFilter::OFF);
    let file_slot = file_layer.with_filter(level_layer);
    let recording = Recording::new();
    let recorder_slot = recording.layer().with_filter(RECORDER_FLOOR);
    let handle = Handle {
        inner: Arc::new(Shared {
            process,
            version,
            file,
            level,
            recorder: recording.recorder,
            written: recording.written,
            state: Mutex::new(State::default()),
        }),
    };
    (handle, vec![Box::new(file_slot), Box::new(recorder_slot)])
}

static GLOBAL: OnceLock<Handle> = OnceLock::new();

/// Set the process's subscriber — once; a second call answers the same
/// handle — and the panic hook that writes a panic as an `error` line and a
/// crash report before the process unwinds. `version` is the binary's, said
/// on every *log started* line and in every report so a file names the
/// build that wrote it.
pub fn install(process: Process, version: &'static str) -> Handle {
    GLOBAL
        .get_or_init(|| {
            let (handle, layers) = build(process, version);
            let stderr = fmt::layer()
                .with_writer(std::io::stderr)
                .with_filter(EnvFilter::from_default_env());
            // A subscriber already owning the process — a test's — keeps it;
            // the handle still answers, its slots simply reach nothing. The
            // hook is the process's either way.
            if tracing_subscriber::registry()
                .with(layers)
                .with(stderr)
                .try_init()
                .is_err()
            {
                tracing::debug!(target: "bisa_log", "another subscriber owns the process; the file reaches nothing");
            }
            install_panic_hook(handle.clone());
            handle
        })
        .clone()
}

/// The process's handle, once [`install`] answered it — for a site with no
/// context to carry one, such as a ctrl-c arm that must say goodbye before
/// it exits.
pub fn current() -> Option<Handle> {
    GLOBAL.get().cloned()
}

/// The folder [`LOG_DIR_ENV`] names, when a parent process set it.
pub fn env_dir() -> Option<PathBuf> {
    std::env::var_os(LOG_DIR_ENV)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

impl Handle {
    pub fn process(&self) -> Process {
        self.inner.process
    }

    pub fn version(&self) -> &'static str {
        self.inner.version
    }

    /// The root — the workspace's `logs/` — once attached.
    pub fn root(&self) -> Option<PathBuf> {
        self.state().root.clone()
    }

    /// The configuration in force.
    pub fn config(&self) -> LogConfig {
        self.state().config
    }

    /// Whether a file is open right now: attached, enabled, the folder made.
    pub fn writing(&self) -> bool {
        self.state().file_open
    }

    /// The flight recorder's entries at this moment, oldest first.
    pub fn recent(&self) -> Vec<Recorded> {
        self.inner.recorder.snapshot()
    }

    /// Give the file layer its folder and configuration. The family's
    /// folder is made when missing; a root whose parent does not exist yet
    /// — a workspace not yet initialised — is remembered and nothing is
    /// written, so `bisa init` leaves no `logs/` behind before the
    /// workspace. The first open sweeps the family's stale run markers,
    /// writes this run's and replays what was said before there was a file.
    pub fn attach(&self, root: impl Into<PathBuf>, config: LogConfig) -> Result<(), AttachError> {
        let root = root.into();
        let parent_exists = root.parent().map(Path::is_dir).unwrap_or(false);
        if !parent_exists {
            tracing::warn!(
                target: "bisa_log",
                root = %root.display(),
                "no log file: the folder's parent does not exist yet"
            );
            self.close_file()?;
            self.remember(Some(root), config);
            return Ok(());
        }
        if !config.enabled {
            self.close_file()?;
            // Off is off: what was said before, and what is said while off,
            // is owed to no later file — unlike words said before a folder
            // existed, which the first open replays.
            self.inner.recorder.forget_unwritten();
            self.inner.written.set(true);
            self.remember(Some(root), config);
            return Ok(());
        }
        let dir = self.inner.process.dir(&root);
        std::fs::create_dir_all(&dir)?;
        let appender = RollingFileAppender::builder()
            .rotation(config.rotation.rotation())
            .filename_prefix(self.inner.process.prefix())
            .filename_suffix(SUFFIX)
            .max_log_files(config.keep_files.max(1))
            .build(&dir)?;
        let first = !self.state().opened;
        if first {
            self.replay(&appender, config.level);
        }
        let layer = fmt::layer()
            .json()
            .flatten_event(true)
            .with_current_span(false)
            .with_span_list(false)
            .with_writer(appender);
        self.inner.file.reload(Some(layer))?;
        self.inner.level.modify(|l| *l = config.filter())?;
        self.inner.written.set(true);
        self.remember(Some(root.clone()), config);
        self.state().file_open = true;
        if first {
            self.first_open(&root);
        }
        tracing::info!(
            target: "bisa_log",
            process = self.inner.process.prefix(),
            version = self.inner.version,
            pid = std::process::id(),
            file = %family_glob(&root, self.inner.process).display(),
            crashes = %crashes_dir(&root).display(),
            // `level` is the line's own; the configured floor gets a
            // name of its own so the two never collide when flattened.
            min_level = %config.level,
            rotation = %config.rotation,
            keep_files = config.keep_files,
            "log started"
        );
        Ok(())
    }

    /// Change the configuration in place — the settings moved. With no
    /// folder attached yet only the configuration is remembered.
    pub fn apply(&self, config: LogConfig) -> Result<(), AttachError> {
        match self.root() {
            Some(root) => self.attach(root, config),
            None => {
                self.remember(None, config);
                Ok(())
            }
        }
    }

    /// The run is ending as it meant to: one `info` line — *log stopped*,
    /// with the reason and the uptime — and this run's marker removed, so
    /// the next start of the family finds nothing to report. Idempotent.
    pub fn goodbye(&self, reason: &str) {
        let (marker, uptime, opened) = {
            let mut state = self.state();
            if state.goodbye_said {
                return;
            }
            state.goodbye_said = true;
            (
                state.marker.take(),
                state.started.elapsed().as_secs(),
                state.opened,
            )
        };
        if opened {
            tracing::info!(
                target: "bisa_log",
                process = self.inner.process.prefix(),
                pid = std::process::id(),
                reason,
                uptime_secs = uptime,
                "log stopped"
            );
        }
        if let Some(marker) = marker {
            if let Err(error) = std::fs::remove_file(&marker) {
                tracing::warn!(target: "bisa_log", file = %marker.display(), %error, "the run marker could not be removed");
            }
        }
    }

    /// Write a crash report under the root's `crashes/`, stamped with this
    /// process, its version and pid, the moment, and the recorder's entries
    /// — for a site that saw a death that is not a panic: the shell's node
    /// that exited. Answers the report's path.
    pub fn report_crash(&self, report: CrashReport) -> Result<PathBuf, ReportError> {
        self.report_crash_at(report, SystemTime::now())
    }

    fn report_crash_at(
        &self,
        mut report: CrashReport,
        at: SystemTime,
    ) -> Result<PathBuf, ReportError> {
        let (root, enabled) = {
            let state = self.state();
            (state.root.clone(), state.config.enabled)
        };
        let root = root.ok_or(ReportError::NoFolder)?;
        if !enabled {
            return Err(ReportError::Off);
        }
        report.process = self.inner.process;
        report.version = self.inner.version.to_string();
        report.pid = std::process::id();
        if report.at.is_empty() {
            report.at = crate::stamp::rfc3339(at);
        }
        if report.recent.is_empty() {
            report.recent = self.recent();
        }
        Ok(crash::write(&root, &report, at)?)
    }

    /// The one moment the process looks around its folder: the family's
    /// stale markers become reports, this run's marker is written.
    fn first_open(&self, root: &Path) {
        let now = SystemTime::now();
        lifecycle::sweep(root, self.inner.process, now);
        let marker = Marker {
            process: self.inner.process,
            version: self.inner.version.to_string(),
            pid: std::process::id(),
            started_at: crate::stamp::rfc3339(now),
        };
        let path = match lifecycle::mark(root, &marker) {
            Ok(path) => Some(path),
            Err(e) => {
                tracing::warn!(target: "bisa_log", "no run marker: {e}");
                None
            }
        };
        let mut state = self.state();
        state.opened = true;
        state.marker = path;
    }

    /// What the recorder heard before there was a file, at or above the
    /// level, written into the appender as the lines they would have been,
    /// each marked `replayed`. Before the layer takes the appender, so the
    /// lines sit above what follows.
    fn replay(&self, appender: &RollingFileAppender, level: LogLevel) {
        let entries = self.inner.recorder.take_unwritten();
        let mut writer = appender.make_writer();
        for entry in entries {
            let Some(said) = level_of_word(&entry.level) else {
                continue;
            };
            if said > level {
                continue;
            }
            let mut line = serde_json::Map::new();
            line.insert("timestamp".into(), entry.at.into());
            line.insert("level".into(), entry.level.into());
            line.insert("target".into(), entry.target.into());
            line.insert("message".into(), entry.message.into());
            for (name, value) in entry.fields {
                line.entry(name).or_insert(serde_json::Value::String(value));
            }
            line.insert("replayed".into(), true.into());
            let mut text = serde_json::Value::Object(line).to_string();
            text.push('\n');
            if writer.write_all(text.as_bytes()).is_err() {
                break;
            }
        }
    }

    fn close_file(&self) -> Result<(), AttachError> {
        self.inner.file.reload(None)?;
        self.inner.level.modify(|l| *l = LevelFilter::OFF)?;
        self.inner.written.set(false);
        self.state().file_open = false;
        Ok(())
    }

    fn remember(&self, root: Option<PathBuf>, config: LogConfig) {
        let mut state = self.state();
        if root.is_some() {
            state.root = root;
        }
        state.config = config;
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.inner.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// The level a recorded line's word names, for the replay's floor.
fn level_of_word(word: &str) -> Option<LogLevel> {
    word.to_ascii_lowercase().parse().ok()
}

/// Write a panic as a crash report and an `error` line naming it, then let
/// the previous hook print it as it would have — the folder gets the facts,
/// the terminal keeps its message. The hook never panics itself: a report
/// that cannot be written is said on stderr, and the whole of it is caught.
fn install_panic_hook(handle: Handle) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()));
        let thread = std::thread::current().name().map(str::to_string);
        let message = panic_message(info.payload());
        let reported = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let at = SystemTime::now();
            let backtrace = std::backtrace::Backtrace::force_capture().to_string();
            let mut report = CrashReport::new(CrashKind::Panic, message.clone())
                .with_location(location.clone())
                .with_thread(thread.clone())
                .with_backtrace(backtrace);
            report.process = handle.process();
            report.pid = std::process::id();
            let name = report.file_name(at);
            tracing::error!(
                target: "panic",
                location = %location.clone().unwrap_or_default(),
                thread = %thread.clone().unwrap_or_default(),
                crash = %name,
                "{message}"
            );
            if let Err(e) = handle.report_crash_at(report, at) {
                eprintln!("bisa-log: no crash report for this panic: {e}");
            }
        }));
        if reported.is_err() {
            eprintln!("bisa-log: the crash report for this panic panicked itself");
        }
        previous(info);
    }));
}

/// What a panic said: its `&str` or `String` payload, else the word
/// `panic`. The one downcast, shared by the hook, the node's panic layer
/// and the engine's task guards, so every surface reads the same sentence.
pub fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "panic".to_string())
}
