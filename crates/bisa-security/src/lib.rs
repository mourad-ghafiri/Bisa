//! Security, as pure functions over text: the **Redactor**, the **Tool &
//! Commands Guard** and the **Auto Classifier**'s contract.
//!
//! Nothing here does I/O. The engine hands this crate a string and a policy
//! and gets a string and a verdict back; where the string came from and what
//! happens to the verdict is the engine's business. That is what lets every
//! rule be tested against a synthetic token and a command that is only ever
//! *matched*, never run.
//!
//! - [`redact`] — rules that recognise a secret, the [`Vault`] that swaps it
//!   for a placeholder (`«secret:github_token:7f3a2c»`) and swaps it back,
//!   and the two walks over text and JSON.
//! - [`guard`] — ordered rules over a tool call's command, paths and name,
//!   first match wins, four verdicts.
//! - [`classify`] — the prompt the classifier is asked and the strict reading
//!   of its one-line answer.
//! - [`builtin`] — the rules that ship: switchable, undeletable, with stable
//!   ids — and the detectors armed from the names of the node's own
//!   environment variables.
//! - [`policy`] — the merge of the built-ins and every settings layer into one
//!   [`SecurityPolicy`], reporting what it could not read instead of failing.
//!
//! The contract the engine holds on top of this crate: a placeholder is
//! restored **only at an execution point on this machine** — a tool input the
//! harness is about to run, a `check` or probe command — and never into
//! anything stored, synced, journaled or sent.

pub mod builtin;
pub mod classify;
pub mod guard;
pub mod net;
pub mod policy;
pub mod redact;

pub use net::{decide_host, host_matches, parse_hosts, HostAllowedBy, HostPolicy, HostVerdict};

pub use classify::{
    message_digest, message_prompt, parse_verdict, subject_digest, MessageSubject, NoVerdict,
    Subject, Verdict as ClassifierVerdict,
};
pub use guard::{Action, Guard, GuardRule, Matcher, RefusedLine, ToolCall, Verdict as RuleVerdict};
pub use policy::{Feature, Problem, SecurityPolicy, SettingsLayer};
pub use redact::{
    has_placeholder, Detector, Origin, Placeholder, RedactRule, Redaction, Redactor, Restored,
    Vault,
};
