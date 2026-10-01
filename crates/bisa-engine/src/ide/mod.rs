//! The Project IDE's engine modules: every filesystem effect the editor, the
//! explorer and the watcher need, owned here so the node stays a parser.
//!
//! See `docs/architecture/ide/01-trust-boundary.md` and `03-files-and-editing.md`.

pub mod connection;
pub mod files;
pub mod git;
pub mod graph;
pub mod index;
pub mod interactive;
pub mod layout;
pub mod review;
pub mod search;
pub mod watch;
