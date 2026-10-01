//! The crate's integration tests, as one binary: one module per file,
//! so the suite links once and `cargo check --tests` compiles it once.
//! Run one module with `just test-module codehost <module>`.

mod bitbucket;
mod fake;
mod gh;
mod github;
mod gitlab;
mod glab;
mod layered;
mod support;
