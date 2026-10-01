//! The crate's integration tests, as one binary: one module per file,
//! so the suite links once and `cargo check --tests` compiles it once.
//! Run one module with `just test-module store <module>`.

mod activity;
mod addons;
mod archive;
mod artifacts;
mod catalog;
mod changes;
mod channels;
mod claims;
mod connectors;
mod conversations;
mod documents;
mod drawings;
mod durability;
mod layout;
mod listening;
mod locking;
mod members;
mod notes;
mod pets;
mod schema_doc;
mod settings;
mod studio;
mod usage;
mod workspace_runs;
