//! The crate's integration tests, as one binary: one module per file, so
//! the suite links once and `cargo check --tests` compiles it once. Every
//! test runs against the loopback stub in `support` or a scripted transport;
//! nothing leaves the machine and no real credential exists anywhere here.

mod support;

mod auth;
mod error;
mod hardening;
mod hosts;
mod oauth;
mod outcome;
mod request;
mod retry;
