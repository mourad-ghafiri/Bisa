//! The crate's integration tests, as one binary: one module per file,
//! so the suite links once and `cargo check --tests` compiles it once.
//! Run one module with `just test-module log <module>`.

mod log;
