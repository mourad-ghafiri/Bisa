//! Language intelligence (ide/10): language servers, supervised and
//! proxied. A leaf crate — `bisa-core` and nothing else of ours.
//!
//! Three small pieces, each tested on its own:
//!
//! - [`codec`] — the JSON-RPC framing (`Content-Length`), incremental.
//! - [`catalog`] — which server for which language: compiled-in presets probed
//!   on the login shell's `PATH`, then user descriptors from settings; the
//!   platform never installs one.
//! - [`uri`] — `file://` URIs on the server's side, root-relative paths on the
//!   editor's, rewritten at the boundary so the webview never learns an
//!   absolute path from a server.
//!
//! [`server::Server`] is one supervised process over stdio: `initialize`,
//! document synchronisation, typed requests with a deadline, notifications
//! on a channel, and a status that turns `Failed` with the reason — never a
//! guess about bytes that did not parse.
//!
//! The client frames JSON-RPC directly over tokio pipes (≈150 lines, tested)
//! and keeps payloads as `serde_json::Value`, for the same reason the code host
//! kept a small typed client: the surface needed here is small and the
//! transport is the risk.

pub mod catalog;
pub mod codec;
pub mod server;
pub mod uri;

#[derive(Debug, thiserror::Error)]
pub enum LspError {
    #[error("could not start the language server: {0}")]
    Spawn(String),
    #[error("the language server broke protocol: {0}")]
    Protocol(String),
    #[error("the language server is not running")]
    Closed,
    #[error("the language server did not answer in time: {0}")]
    Timeout(String),
    #[error("the language server refused: {message} (code {code})")]
    ServerError { code: i64, message: String },
    #[error("no language server for {0}")]
    NoServer(String),
}

pub type LspResult<T> = Result<T, LspError>;
