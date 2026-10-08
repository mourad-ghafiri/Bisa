//! The crate's integration tests, as one binary: one module per file,
//! so the suite links once and `cargo check --tests` compiles it once.
//! Run one module with `just test-module node <module>`.

mod a2a;
mod addons;
mod auth;
mod bodies;
mod cache;
mod changes;
mod collab;
mod connectors;
mod conversations;
mod decisions;
mod drawings;
mod effort;
mod events;
mod files;
mod harness_usage;
mod ide;
mod layering;
mod logs;
mod mobile_development;
mod network;
mod node;
mod notes_git;
mod project_delete;
mod readiness;
mod reference;
mod resilience;
mod retire;
mod routes;
mod runs;
mod security;
mod sessions;
mod settings;
mod ssh;
mod updates;
