//! Every word the platform says to a person, in the person's language
//! ([17 — Internationalisation](../../../docs/architecture/17-internationalisation.md)).
//!
//! The platform speaks in [`Text`]s — a message id and its arguments
//! (`bisa_core::text`). This crate is where a `Text` becomes a sentence:
//!
//! - [`Locale`] — a language identifier, parsed, read from the environment
//!   (`LC_ALL` · `LC_MESSAGES` · `LANG`) or an `Accept-Language` header, and
//!   negotiated against the languages the platform ships ([`AVAILABLE`]).
//! - [`Catalog`] — the messages, one Fluent bundle per shipped language,
//!   built once from the `.ftl` files under `locales/<lang>/` that the crates
//!   own ([`Namespace`]); the desktop reads the same files with its own
//!   runtime, so a message is written once and read by both.
//! - [`render`] — the sentence for a `Text` in a locale; a message the
//!   catalog lacks renders as its id, visibly, and is said once in the log.
//! - [`process`] — one language for one process, the CLI's: set once at
//!   start, [`say`] renders a `Text` in it from anywhere.
//!
//! What is never rendered here: a person's own content (titles, message
//! bodies, commit messages), what is for a model (prompts, tool descriptions,
//! intake refusals) and what is for a log — those are not `Text`s.
//!
//! The ratchet ([`ratchet`]) is the guard that holds the migration: how many
//! bare sentences remain in the node's and the CLI's sources, per file, a
//! number that may only go down.

pub mod catalog;
pub mod locale;
pub mod process;
pub mod ratchet;

pub use catalog::{Catalog, Namespace};
pub use locale::{Locale, AVAILABLE, DEFAULT};
pub use process::say;

use bisa_core::Text;

/// The sentence for `text` in `locale` — the id itself when the catalog has
/// no such message, so a miss is seen and never silent.
pub fn render(locale: &Locale, text: &Text) -> String {
    Catalog::global().render(locale, text)
}

/// One attribute of a message — a setting's `.help`, a widget's `.placeholder`.
pub fn attribute(locale: &Locale, id: &str, attribute: &str, text: &Text) -> Option<String> {
    Catalog::global().attribute(locale, id, attribute, text)
}

/// The English sentence — what a docs generator writes and a test compares.
pub fn english(text: &Text) -> String {
    render(&Locale::english(), text)
}

/// The message id of a catalog entry's display fields: `catalog-<kind>-<slug>`
/// (`Namespace::Content`); its name is the value, `.description` and a pet's
/// `.tagline` its attributes.
pub fn content_id(kind: &str, slug: &str) -> String {
    format!("catalog-{kind}-{slug}")
}

/// A catalog entry's display field in `locale`, or the shipped file's own
/// text when no translation ships (`Catalog::content`).
pub fn content(locale: &Locale, id: &str, attribute: Option<&str>, fallback: &str) -> String {
    Catalog::global().content(locale, id, attribute, fallback)
}
