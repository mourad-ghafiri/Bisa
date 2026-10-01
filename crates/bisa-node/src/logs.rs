//! The diagnostic log's routes: where the files are and what is there, for
//! Settings › Node › Logging and `bisa logs`, and one crash report
//! whole. A read — the engine lists the folder the store names — and
//! nothing here tails a file: a log is a file a person opens or attaches to
//! a bug report, never a stream the node serves. A crash report is the one
//! thing read back, because it is what a person needs first.

use crate::dto::{CrashReportView, CrashSummaryView, LogFamilyView, LogFileView, LogsView};
use crate::route_docs::RouteDoc;
use crate::{ApiError, Shared};
use axum::extract::{Path as AxPath, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};

pub(crate) fn routes() -> Router<Shared> {
    Router::new()
        .route("/logs", get(logs))
        .route("/logs/crashes/{name}", get(crash))
}

fn file_view(f: bisa_engine::logging::LogFile) -> LogFileView {
    LogFileView {
        name: f.name,
        bytes: f.bytes,
        modified_at: f.modified_at,
    }
}

/// The folder, every family's files newest first, the crash reports, the
/// bytes they hold together and the newest report in a line.
async fn logs(State(state): State<Shared>) -> Json<LogsView> {
    let inner = state.engine.inner();
    let listing = bisa_engine::logging::listing(inner);
    let bytes = listing.bytes();
    let families = listing
        .families
        .into_iter()
        .map(|f| LogFamilyView {
            process: f.process.prefix().to_string(),
            dir: f.dir.display().to_string(),
            files: f.files.into_iter().map(file_view).collect(),
        })
        .collect();
    let crashes = listing.crashes.into_iter().map(file_view).collect();
    let latest_crash =
        bisa_engine::logging::latest_crash(inner).map(|(name, r)| CrashSummaryView {
            name,
            process: r.process.prefix().to_string(),
            kind: r.kind.as_str().to_string(),
            at: r.at,
            message: r.message,
        });
    let dir = bisa_engine::logging::dir(inner);
    Json(LogsView {
        crashes_dir: bisa_log::crashes_dir(&dir).display().to_string(),
        dir: dir.display().to_string(),
        families,
        crashes,
        bytes,
        latest_crash,
    })
}

/// One crash report by its file name. A name that is not a crash name is
/// refused before any path is joined; a report that is not there is 404.
async fn crash(
    State(state): State<Shared>,
    AxPath(name): AxPath<String>,
) -> Result<Json<CrashReportView>, ApiError> {
    if !bisa_log::is_crash_name(&name) {
        return Err(ApiError::text(
            StatusCode::NOT_FOUND,
            bisa_core::text!(
                "error-node-logs-not-crash-report-s-name",
                name = format!("{name:?}")
            ),
        ));
    }
    bisa_engine::logging::crash(state.engine.inner(), &name)
        .map(|r| Json(CrashReportView::from(r)))
        .ok_or_else(|| {
            ApiError::text(
                StatusCode::NOT_FOUND,
                bisa_core::text!("error-node-logs-no-crash-report", name = name.to_string()),
            )
        })
}

/// The routes this module mounts, for `bisa-node --bin api-docs` and the
/// route test.
pub const ROUTES: &[RouteDoc] = &[
    RouteDoc {
        method: "GET",
        path: "/logs",
        summary: "The diagnostic log on this machine: the folder (`dir`), one entry per process family (`families[]`: `process`, `dir`, `files[]` with `name`, `bytes`, `modified_at`, newest first), the crash reports' folder (`crashes_dir`) and the reports in it (`crashes[]`, newest first), the bytes they hold together and the newest report in a line (`latest_crash`: `name`, `process`, `kind`, `at`, `message`). The files are what the node, the CLI, the MCP servers and the desktop wrote about themselves under the `logging.*` settings; nothing is sent anywhere.",
    },
    RouteDoc {
        method: "GET",
        path: "/logs/crashes/{name}",
        summary: "One crash report whole, by its file name: the process, the build, the pid, the moment, the kind (`panic` · `abrupt_end` · `child_exit`), the words, a panic's location, thread and backtrace, a supervised child's exit code, signal and last stderr lines, and the flight recorder's entries at the moment (`recent[]`, oldest first). A name that is not a report's is 404.",
    },
];
