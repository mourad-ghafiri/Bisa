//! Outside platforms' HTTP APIs, called through one declarative shape.
//!
//! A **connector** is a definition the domain owns: a base URL, the hosts it
//! may reach, an auth scheme, and operations — a method, a path, a query, a
//! body, the parameters each takes. This crate owns what happens between that
//! definition and the wire: binding a step's parameters by kind, rendering the
//! definition's `{account.…}` and `{params.…}` placeholders into a URL and a
//! body, applying the account's credential, refusing a host the definition
//! does not name, sending with one retry policy, refreshing an OAuth2 token
//! before it expires, and turning the answer into an [`Outcome`] the run
//! machine records.
//!
//! It depends on nothing of ours, like `bisa-codehost`. Everything it
//! needs from the platform arrives through traits the engine implements:
//! [`Credentials`] (a secret by name, a refreshed token to keep), [`Clock`],
//! [`Entropy`], [`HostJudge`] (the workspace's own allow and deny lists),
//! [`Files`] (the bytes a `file` parameter names, from the run's checkout)
//! and [`HttpTransport`] (so a test runs against a loopback stub). It never logs,
//! stores or returns a credential — [`Secret`] cannot be printed, every error
//! it builds is [`scrub`]bed of the strings it exposed — never follows a
//! redirect, never writes to disk, and never reaches a host the definition
//! did not declare.

pub mod auth;
pub mod body;
pub mod breaker;
pub mod client;
pub mod creds;
pub mod error;
pub mod files;
pub mod hosts;
pub mod http;
pub mod oauth;
pub mod outcome;
pub mod request;
pub mod retry;
pub mod spec;
pub mod template;

pub use breaker::{Breaker, COOLDOWN_SECS, OPEN_AFTER};
pub use client::Client;
pub use creds::{
    AccountRef, Clock, Credential, Credentials, Entropy, Field, OsEntropy, Secret, Stored,
    SystemClock, TokenSet,
};
pub use error::{scrub, ConnectorError};
pub use files::{FileData, Files, NoFiles, MAX_FILE_BYTES};
pub use hosts::{AllowAll, HostJudge};
pub use http::{HttpTransport, Request, ReqwestTransport, Response, TransportError};
pub use oauth::{authorize_url, exchange, refresh, Authorize};
pub use outcome::Outcome;
pub use retry::{Next, RetryPolicy};
pub use spec::{
    AuthSpec, CallBody, CallSpec, Idempotency, JwtAlg, KeyPlace, Method, Paging, ParamKind,
    ParamSpec, Part, PartSource,
};
pub use template::{Encode, Values};
