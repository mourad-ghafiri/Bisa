//! The Decision-Making Agent's providers, behind one port.
//!
//! The contract is the domain's ([`bisa_core::decision`]): typed questions
//! against a state in, `{ model, answers, usage }` out. This crate owns what
//! stands between a caller and a model that answers it:
//!
//! - [`DecisionProvider`] — the port. One method, one shape, whoever answers.
//! - [`SystemOneProvider`] — a model trained for calibrated decisions, over its
//!   HTTP wire: Jev at TypeSafe AI's API, or any other RLCD model at an
//!   endpoint a person named.
//! - [`PromptedProvider`] — a generative model, asked through an [`Asker`] the
//!   engine implements (a harness with one of its models, or an agent) and held
//!   to the same shape.
//! - [`Checked`], [`Retrying`], [`Bounded`] — the three decorators every
//!   provider is wrapped in: the contract check, the retry budget, the one
//!   deadline. A response that breaks the contract is an error, never a guess.
//! - [`build`] — the one place a provider kind is matched; switching provider
//!   is one setting.
//!
//! Nothing here reads a setting, a keystore or a clock of its own: the engine
//! hands in [`DecisionSettings`] and [`Ports`]. Nothing here redacts either —
//! the engine redacts a request before it reaches a provider, because an HTTP
//! provider is an outside service. A test substitutes the transport and the
//! asker, and nothing leaves the machine.

pub mod error_text;
pub mod factory;
pub mod prompted;
pub mod provider;
pub mod resilient;
pub mod scripted;
pub mod system_one;

pub use factory::{build, DecisionSettings, KeySource, NoKeys, Ports, RlcdAuth};
pub use prompted::{AskTarget, Asked, Asker, PromptedProvider};
pub use provider::{DecisionProvider, ProviderDescriptor, ProviderError};
pub use resilient::{Bounded, Checked, RetryBudget, Retrying};
pub use scripted::ScriptedProvider;
pub use system_one::{SystemOneProvider, JEV_ENDPOINT, SYSTEM_ONE_PATH};
