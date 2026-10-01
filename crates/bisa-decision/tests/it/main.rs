//! The crate's integration tests, as one binary: one module per file. Every
//! test answers from a scripted transport or a scripted asker; nothing leaves
//! the machine, no harness runs and no real key exists anywhere here.

mod support;

mod factory;
mod prompted;
mod resilient;
mod system_one;
