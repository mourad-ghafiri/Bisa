//! A sentence as data: what the platform says to a person, before any
//! language has been chosen ([17 — Internationalisation](../../../docs/architecture/17-internationalisation.md)).
//!
//! The platform authors prose in many places — a refusal, a setting's
//! label, a validation problem, a step's summary — and a person reads it in
//! their language at one of the edges: the desktop, the CLI, or the node's
//! own error body. So the prose never travels as a sentence. It travels as a
//! [`Text`]: the **id** of a message in the catalog (`locales/<lang>/*.ftl`)
//! and the **arguments** the message interpolates. Whoever reads it renders
//! it (`bisa-i18n` in the crates, `desktop/src/i18n/l10n.mjs` in the
//! webview); the same id and arguments give the same sentence in every
//! language that has one, and the id itself where none does.
//!
//! An argument is a scalar — a string, an integer, a number — because that
//! is what a Fluent message selects on and formats. A step id, a path, a
//! person's own title ride as arguments and are never translated: the
//! template is the platform's, the values are the world's.
//!
//! **English is still spoken here, plainly.** A `Text` prints (`Display`) as
//! its English sentence — the log's, a test's, an agent's — rendered by
//! [`plain`]: the compiled-in English catalog read as lines, `{ $name }`
//! placeables substituted, a select expression answered with its default
//! variant. No Fluent, no locale: the developer's English, exact for every
//! message written as one line, which is what a refusal is. A person's
//! language is the edge's business (`bisa-i18n`, the desktop).
//!
//! [`Localize`] is what an error type implements to say itself as a `Text`:
//! one arm per variant, generated from the `#[error]` string it already
//! carries and held equal to the catalog by `crates/bisa-i18n/tests/it/catalog.rs`.
//!
//! This module has no dependency and does no I/O (`include_str!` is the
//! compiler's); rendering for a language is not its job.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt;

/// One argument a message interpolates.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Arg {
    /// A whole number — a count a message may select a plural on.
    Int(i64),
    /// A number with a fraction.
    Num(f64),
    /// A string: an id, a name, a path — the world's word, shown as it is.
    Str(String),
}

/// Two arguments are equal when they would render the same: a number by its
/// bits, so a `Text` can be a field of a value that is `Eq` (a message body,
/// a problem) — `f64` alone is not.
impl PartialEq for Arg {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Arg::Int(a), Arg::Int(b)) => a == b,
            (Arg::Num(a), Arg::Num(b)) => a.to_bits() == b.to_bits(),
            (Arg::Str(a), Arg::Str(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for Arg {}

impl From<&str> for Arg {
    fn from(v: &str) -> Self {
        Arg::Str(v.to_string())
    }
}
impl From<String> for Arg {
    fn from(v: String) -> Self {
        Arg::Str(v)
    }
}
impl From<&String> for Arg {
    fn from(v: &String) -> Self {
        Arg::Str(v.clone())
    }
}
impl From<Cow<'_, str>> for Arg {
    fn from(v: Cow<'_, str>) -> Self {
        Arg::Str(v.into_owned())
    }
}
impl From<i64> for Arg {
    fn from(v: i64) -> Self {
        Arg::Int(v)
    }
}
impl From<i32> for Arg {
    fn from(v: i32) -> Self {
        Arg::Int(v as i64)
    }
}
impl From<i16> for Arg {
    fn from(v: i16) -> Self {
        Arg::Int(v as i64)
    }
}
impl From<i8> for Arg {
    fn from(v: i8) -> Self {
        Arg::Int(v as i64)
    }
}
impl From<u16> for Arg {
    fn from(v: u16) -> Self {
        Arg::Int(v as i64)
    }
}
impl From<u8> for Arg {
    fn from(v: u8) -> Self {
        Arg::Int(v as i64)
    }
}
impl From<f32> for Arg {
    fn from(v: f32) -> Self {
        Arg::Num(v as f64)
    }
}
impl From<u32> for Arg {
    fn from(v: u32) -> Self {
        Arg::Int(v as i64)
    }
}
impl From<u64> for Arg {
    fn from(v: u64) -> Self {
        Arg::Int(i64::try_from(v).unwrap_or(i64::MAX))
    }
}
impl From<usize> for Arg {
    fn from(v: usize) -> Self {
        Arg::Int(i64::try_from(v).unwrap_or(i64::MAX))
    }
}
impl From<f64> for Arg {
    fn from(v: f64) -> Self {
        Arg::Num(v)
    }
}
impl From<bool> for Arg {
    /// A switch a message selects on: `[true]` / `*[false]`.
    fn from(v: bool) -> Self {
        Arg::Str(v.to_string())
    }
}

/// A message of the catalog with its arguments — the platform's sentence,
/// not yet in any language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Text {
    /// The message id, as `locales/<lang>/*.ftl` spells it: kebab-case, area-prefixed.
    pub id: Cow<'static, str>,
    /// The arguments, by the names the message's `{ $name }` placeables use.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub args: BTreeMap<Cow<'static, str>, Arg>,
}

impl Text {
    /// A message with no arguments.
    pub fn new(id: impl Into<Cow<'static, str>>) -> Self {
        Text {
            id: id.into(),
            args: BTreeMap::new(),
        }
    }

    /// The same message with one more argument.
    pub fn arg(mut self, name: impl Into<Cow<'static, str>>, value: impl Into<Arg>) -> Self {
        self.args.insert(name.into(), value.into());
        self
    }

    /// One argument's value, by name.
    pub fn get(&self, name: &str) -> Option<&Arg> {
        self.args.get(name)
    }
}

impl fmt::Display for Text {
    /// The English sentence, plainly rendered — what a log and a test read.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&plain::render(self))
    }
}

impl fmt::Display for Arg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Arg::Int(i) => write!(f, "{i}"),
            Arg::Num(n) => write!(f, "{n}"),
            Arg::Str(s) => f.write_str(s),
        }
    }
}

/// What can say itself to a person: an error type, one message per variant.
pub trait Localize {
    /// The message and its arguments — never a sentence.
    fn text(&self) -> Text;
}

/// The English catalog, read plainly: the crate-owned files the platform's
/// refusals and sentences live in, as the developer's `Display`.
pub mod plain {
    use super::{Arg, Text};
    use std::collections::HashMap;
    use std::sync::LazyLock;

    /// The English files a `Text` of the crates may name.
    const SOURCES: &[&str] = &[
        include_str!("../../../locales/en/errors.ftl"),
        include_str!("../../../locales/en/problems.ftl"),
        include_str!("../../../locales/en/engine.ftl"),
        include_str!("../../../locales/en/settings.ftl"),
        include_str!("../../../locales/en/cli.ftl"),
    ];

    /// `id = value` per message, the value with its continuation lines
    /// joined; attributes and comments left out.
    fn messages() -> &'static HashMap<String, String> {
        static MESSAGES: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
            let mut out = HashMap::new();
            for source in SOURCES {
                let mut current: Option<(String, String)> = None;
                for line in source.lines() {
                    if line.starts_with('#') {
                        continue;
                    }
                    let is_entry = line
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_ascii_alphabetic() || c == '-');
                    if is_entry {
                        if let Some((id, value)) = current.take() {
                            out.insert(id, value);
                        }
                        if let Some((id, value)) = line.split_once('=') {
                            current = Some((id.trim().to_string(), value.trim().to_string()));
                        }
                    } else if let Some((_, value)) = current.as_mut() {
                        let t = line.trim();
                        if t.starts_with('.') {
                            // An attribute: not the value's.
                            if let Some((id, value)) = current.take() {
                                out.insert(id, value);
                            }
                        } else if !t.is_empty() {
                            if !value.is_empty() {
                                value.push(' ');
                            }
                            value.push_str(t);
                        }
                    }
                }
                if let Some((id, value)) = current.take() {
                    out.insert(id, value);
                }
            }
            out
        });
        &MESSAGES
    }

    /// The default variant of a select expression's body: `*[key] words`.
    /// A select expression's variants: `[key] words` pairs and which is the default.
    fn variants(body: &str) -> Vec<(bool, String, String)> {
        let mut out = Vec::new();
        let mut rest = body;
        while let Some(open) = rest.find('[') {
            let default = open > 0 && rest.as_bytes()[open - 1] == b'*';
            let Some(close) = rest[open..].find(']') else {
                break;
            };
            let key = rest[open + 1..open + close].trim().to_string();
            let after = &rest[open + close + 1..];
            // The words run to the next variant's `[` or `*[`, or to the end.
            let next = after.find('[').unwrap_or(after.len());
            let words = after[..next]
                .trim()
                .trim_end_matches(|c: char| c == '*' || c.is_whitespace())
                .trim()
                .to_string();
            out.push((default, key, words));
            // Keep the character before the next `[`: a `*` there marks the default variant.
            rest = &after[next.saturating_sub(1)..];
        }
        out
    }

    /// Whether a variant key answers an argument: the number itself, the
    /// English plural category (`one` for 1, `other` else), or the word.
    fn matches(key: &str, arg: &Arg) -> bool {
        match arg {
            Arg::Int(i) => key == i.to_string() || (key == "one" && *i == 1),
            Arg::Num(n) => key == n.to_string() || (key == "one" && *n == 1.0),
            Arg::Str(s) => key == s,
        }
    }

    /// The variant a select expression answers with for `text`: the one its
    /// argument matches, else the default.
    fn select(body: &str, selector: &str, text: &Text) -> String {
        let all = variants(body);
        let chosen = text.get(selector).and_then(|arg| {
            all.iter()
                .find(|(_, key, _)| key != "other" && matches(key, arg))
        });
        let fallback = all.iter().find(|(default, _, _)| *default);
        chosen
            .or(fallback)
            .map(|(_, _, words)| words.clone())
            .unwrap_or_default()
    }

    /// One placeable's words: a variable, a string literal, or a select's answer.
    fn placeable(inner: &str, text: &Text) -> String {
        let t = inner.trim();
        if let Some(var) = t.strip_prefix('$') {
            let name = var.trim();
            if let Some(arrow) = name.find("->") {
                // `$n -> [one] … *[other] …`: the variant the argument answers, its own placeables substituted.
                let selector = name[..arrow].trim();
                let words = select(&name[arrow + 2..], selector, text);
                return substitute(&words, text);
            }
            return text
                .get(name)
                .map(Arg::to_string)
                .unwrap_or_else(|| format!("{{ ${name} }}"));
        }
        if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') {
            return t[1..t.len() - 1].to_string();
        }
        format!("{{ {t} }}")
    }

    /// `{ … }` placeables substituted, braces balanced.
    fn substitute(value: &str, text: &Text) -> String {
        let mut out = String::with_capacity(value.len());
        let mut rest = value;
        while let Some(open) = rest.find('{') {
            out.push_str(&rest[..open]);
            let after = &rest[open + 1..];
            let mut depth = 1;
            let mut close = None;
            let mut quoted = false;
            for (i, c) in after.char_indices() {
                match c {
                    // A brace inside a string literal (`{"{"}`) is a character, not a placeable.
                    '"' => quoted = !quoted,
                    _ if quoted => {}
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            close = Some(i);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let Some(close) = close else {
                out.push_str(&rest[open..]);
                return out;
            };
            out.push_str(&placeable(&after[..close], text));
            rest = &after[close + 1..];
        }
        out.push_str(rest);
        out
    }

    /// The English sentence for `text`; an id the catalog lacks is shown as
    /// itself with its arguments, so a miss is read, not hidden.
    pub fn render(text: &Text) -> String {
        match messages().get(text.id.as_ref()) {
            Some(value) => substitute(value, text),
            None => {
                if text.args.is_empty() {
                    text.id.to_string()
                } else {
                    let args: Vec<String> =
                        text.args.iter().map(|(k, v)| format!("{k}={v}")).collect();
                    format!("{} [{}]", text.id, args.join(", "))
                }
            }
        }
    }

    /// Whether the English catalog has a message `id`.
    pub fn has(id: &str) -> bool {
        messages().contains_key(id)
    }

    /// The select rule, for the module's tests.
    #[cfg(test)]
    pub(super) fn select_for_test(body: &str, selector: &str, text: &Text) -> String {
        select(body, selector, text)
    }
}

/// A [`Text`] in one expression: `text!("error-goal-not-found", id = goal.id)`.
/// The argument names are the message's placeable names, spelt as identifiers.
#[macro_export]
macro_rules! text {
    ($id:literal) => {
        $crate::text::Text::new($id)
    };
    ($id:literal, $($name:ident = $value:expr),+ $(,)?) => {{
        let mut t = $crate::text::Text::new($id);
        $( t = t.arg(stringify!($name), $value); )+
        t
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_text_is_an_id_and_named_scalar_arguments_on_the_wire() {
        let t = text!(
            "error-goal-not-found",
            id = "g1",
            tries = 3usize,
            ratio = 0.5
        );
        assert_eq!(t.id, "error-goal-not-found");
        assert_eq!(t.get("id"), Some(&Arg::Str("g1".into())));
        assert_eq!(t.get("tries"), Some(&Arg::Int(3)));
        assert_eq!(t.get("ratio"), Some(&Arg::Num(0.5)));
        let json = serde_json::to_value(&t).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "id": "error-goal-not-found", "args": { "id": "g1", "ratio": 0.5, "tries": 3 } })
        );
        let back: Text = serde_json::from_value(json).unwrap();
        assert_eq!(back, t);
    }

    #[test]
    fn a_text_prints_as_its_plain_english_and_a_miss_as_its_id_with_its_arguments() {
        // A message the catalog lacks: the id, and the arguments where there are any.
        assert_eq!(
            text!("nothing-by-this-id").to_string(),
            "nothing-by-this-id"
        );
        assert_eq!(
            text!("nothing-by-this-id", n = 2, who = "ada").to_string(),
            "nothing-by-this-id [n=2, who=ada]"
        );
        assert!(!plain::has("nothing-by-this-id"));
    }

    #[test]
    fn a_select_answers_by_its_argument_and_falls_back_to_its_default() {
        let body = " [0] nothing [one] one thing *[other] { $n } things ";
        assert_eq!(
            plain::select_for_test(body, "n", &text!("x", n = 1)),
            "one thing"
        );
        assert_eq!(
            plain::select_for_test(body, "n", &text!("x", n = 0)),
            "nothing"
        );
        assert_eq!(
            plain::select_for_test(body, "n", &text!("x", n = 3)),
            "{ $n } things"
        );
        assert_eq!(
            plain::select_for_test(body, "n", &text!("x")),
            "{ $n } things",
            "no argument: the default"
        );
        let words = " [future] in { $span } *[past] { $span } ";
        assert_eq!(
            plain::select_for_test(words, "direction", &text!("x", direction = "future")),
            "in { $span }"
        );
        assert_eq!(
            plain::select_for_test(words, "direction", &text!("x", direction = "past")),
            "{ $span }"
        );
    }

    #[test]
    fn no_arguments_is_no_args_field_and_a_switch_is_a_word() {
        let t = text!("cli-done");
        assert_eq!(serde_json::to_string(&t).unwrap(), r#"{"id":"cli-done"}"#);
        let back: Text = serde_json::from_str(r#"{"id":"cli-done"}"#).unwrap();
        assert!(back.args.is_empty());
        assert_eq!(Arg::from(true), Arg::Str("true".into()));
        assert_eq!(
            Arg::from(u64::MAX),
            Arg::Int(i64::MAX),
            "clamped, never wrapped"
        );
    }
}
