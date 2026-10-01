//! The platform's diagnostic log — what a process writes about itself so a
//! bug can be read back from a file on this machine.
//!
//! One `tracing` subscriber per process, in three layers:
//!
//! - **stderr**, filtered by `RUST_LOG` exactly as before this crate existed
//!   (`EnvFilter::from_default_env()` — nothing without the variable but
//!   errors), for a person running a terminal;
//! - **the file**, `<logs dir>/<process>/<process>.<date>.jsonl` — one JSON
//!   object per line, written synchronously (one `write` per event, nothing
//!   buffered, so a process that aborts on a panic loses no line), rotated
//!   hourly or daily, the oldest files pruned past a count — behind two
//!   reload slots so the level and the rotation follow a settings change
//!   without a restart;
//! - **the flight recorder**, the last events at `debug` and above in
//!   memory, written nowhere until something dies.
//!
//! A death is a **crash report** under `<logs dir>/crashes/`: a panic, from
//! the hook, with its location, thread, backtrace and the recorder's
//! entries; a run that ended without a goodbye — a signal, an abort, a
//! stack overflow — found by the next start of the same family through the
//! **run marker** it left under `<logs dir>/runs/`; a supervised child that
//! exited, reported by its parent. `Handle::goodbye` is the clean end.
//!
//! The crate knows the **folder** (`files.rs`) and the **words** the
//! settings speak (`config.rs`); it does not know where the workspace is —
//! the root is handed in by whoever owns it (`bisa-store`'s `paths.rs`
//! for the node and the CLI, `bisa paths` for the desktop shell) — and
//! it reads no setting: an engine or a shell resolves the settings and calls
//! [`Handle::apply`].
//!
//! Nothing here sends anything anywhere. A log line carries ids, kinds,
//! statuses, durations and a typed error's sentence; the sites that call the
//! macros never format a prompt, a message body, a token or an environment
//! value (`docs/architecture/11-security.md`). A report carries those same
//! lines, a backtrace and a child's stderr tail.

mod config;
mod crash;
mod files;
mod install;
mod lifecycle;
mod recorder;
mod stamp;

pub use config::{ConfigError, LogConfig, LogLevel, LogRotation};
pub use crash::{
    latest as latest_crash, read as read_crash, write as write_crash, ChildExit, CrashKind,
    CrashReport, KEEP_CRASHES,
};
pub use files::{
    crashes_dir, is_crash_name, is_log_name, is_marker_name, list, runs_dir, Family, Listing,
    LogFile, Process, CRASHES, REPORT_SUFFIX, RUNS, SUFFIX,
};
pub use install::{
    build, current, env_dir, install, panic_message, AttachError, FileSlot, Handle, Layers,
    RecorderSlot, ReportError, LOG_DIR_ENV,
};
pub use lifecycle::{alive, sweep, Marker, Stale};
pub use recorder::{Recorded, RECORDER_CAPACITY, RECORDER_FLOOR};
pub use stamp::rfc3339;
