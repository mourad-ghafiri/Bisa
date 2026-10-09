//! The messages: one Fluent bundle per shipped language, built once from the
//! `.ftl` files the crates own, and the rendering of a `Text` against it.
//!
//! The files are compiled in (`include_str!`), so a node or a CLI carries its
//! languages and reads nothing at run time; the same files sit under
//! `locales/` for the desktop's own runtime to read. Isolation marks are off
//! (`set_use_isolating(false)`): a placeable is not wrapped in FSI/PDI, so a
//! sentence is exactly its characters — what a test compares and a terminal
//! prints — and direction is the document's business.

use crate::locale::{Locale, AVAILABLE};
use bisa_core::{Arg, Text};
use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource, FluentValue};
use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, Mutex};

/// The catalog files the crates own, under `locales/<lang>/`. The desktop's
/// (`locales/<lang>/desktop/*.ftl`) are the webview's alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Namespace {
    /// Settings: `setting-<key>` with `.help` and `.choice-<value>`.
    Settings,
    /// Refusals: `error-<crate>-<variant>` and the node's ad hoc ones.
    Errors,
    /// Workflow validation: `problem-<kind>`.
    Problems,
    /// The engine's own sentences: a step's summary, a guard's reason, a run's stop.
    Engine,
    /// The CLI's lines and its help.
    Cli,
    /// The catalog's display fields — an entry's name and description, a
    /// pet's tagline — by id, with the shipped file's text as the fallback:
    /// `catalog-<kind>-<slug>`, `.description`, `.tagline`. English ships
    /// no entries here; the files are the English.
    Content,
}

impl Namespace {
    /// Every namespace, in the order the files are added to a bundle.
    pub const ALL: [Namespace; 6] = [
        Namespace::Settings,
        Namespace::Errors,
        Namespace::Problems,
        Namespace::Engine,
        Namespace::Cli,
        Namespace::Content,
    ];

    /// The file name under `locales/<lang>/`.
    pub fn file(self) -> &'static str {
        match self {
            Namespace::Settings => "settings.ftl",
            Namespace::Errors => "errors.ftl",
            Namespace::Problems => "problems.ftl",
            Namespace::Engine => "engine.ftl",
            Namespace::Cli => "cli.ftl",
            Namespace::Content => "catalog.ftl",
        }
    }

    /// The file's text for a shipped language, compiled in. A language is
    /// shipped whole: every namespace, or the build does not know it.
    pub fn source(self, tag: &str) -> Option<&'static str> {
        match (tag, self) {
            ("en", Namespace::Settings) => Some(include_str!("../../../locales/en/settings.ftl")),
            ("en", Namespace::Errors) => Some(include_str!("../../../locales/en/errors.ftl")),
            ("en", Namespace::Problems) => Some(include_str!("../../../locales/en/problems.ftl")),
            ("en", Namespace::Engine) => Some(include_str!("../../../locales/en/engine.ftl")),
            ("en", Namespace::Cli) => Some(include_str!("../../../locales/en/cli.ftl")),
            ("en", Namespace::Content) => Some(include_str!("../../../locales/en/catalog.ftl")),
            _ => None,
        }
    }
}

type Bundle = FluentBundle<FluentResource>;

/// The messages of every shipped language.
pub struct Catalog {
    bundles: HashMap<String, Bundle>,
}

/// The ids said once already: a missing message is one warning, not one per paint.
static SAID: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

fn bundle_for(tag: &str, sources: &[&str]) -> Result<Bundle, Vec<String>> {
    let id = tag
        .parse()
        .map_err(|e| vec![format!("{tag}: not a language identifier: {e}")])?;
    let mut bundle = Bundle::new_concurrent(vec![id]);
    bundle.set_use_isolating(false);
    let mut faults = Vec::new();
    for source in sources {
        let resource = match FluentResource::try_new(source.to_string()) {
            Ok(r) => r,
            Err((r, errors)) => {
                faults.extend(errors.iter().map(|e| format!("{tag}: {e:?}")));
                r
            }
        };
        if let Err(errors) = bundle.add_resource(resource) {
            faults.extend(errors.iter().map(|e| format!("{tag}: {e}")));
        }
    }
    if faults.is_empty() {
        Ok(bundle)
    } else {
        Err(faults)
    }
}

fn fluent_args(text: &Text) -> FluentArgs<'_> {
    let mut args = FluentArgs::new();
    for (name, value) in &text.args {
        let v: FluentValue = match value {
            Arg::Int(i) => FluentValue::from(*i),
            Arg::Num(n) => FluentValue::from(*n),
            Arg::Str(s) => FluentValue::from(s.as_str()),
        };
        args.set(name.as_ref(), v);
    }
    args
}

impl Catalog {
    /// The shipped catalog, built once from the compiled-in files.
    pub fn global() -> &'static Catalog {
        static GLOBAL: LazyLock<Catalog> = LazyLock::new(|| {
            Catalog::shipped().unwrap_or_else(|faults| {
                // LCOV_EXCL_START: the shipped files parse: every_catalog_file_parses_as_fluent holds each one
                panic!("the shipped catalog does not parse:\n{}", faults.join("\n"))
                // LCOV_EXCL_STOP
            })
        });
        &GLOBAL
    }

    /// Every shipped language from its compiled-in namespaces.
    pub fn shipped() -> Result<Catalog, Vec<String>> {
        let mut bundles = HashMap::new();
        let mut faults = Vec::new();
        for tag in AVAILABLE {
            let sources: Vec<&str> = Namespace::ALL
                .iter()
                .filter_map(|ns| ns.source(tag))
                .collect();
            match bundle_for(tag, &sources) {
                Ok(b) => {
                    bundles.insert(tag.to_string(), b);
                }
                Err(f) => faults.extend(f), // LCOV_EXCL_LINE: the shipped files parse: every_catalog_file_parses_as_fluent holds each one
            }
        }
        if faults.is_empty() {
            Ok(Catalog { bundles })
        } else {
            Err(faults) // LCOV_EXCL_LINE: the shipped files parse: every_catalog_file_parses_as_fluent holds each one
        }
    }

    /// One language from the given sources — a test's, or a tool's.
    pub fn from_sources(tag: &str, sources: &[&str]) -> Result<Catalog, Vec<String>> {
        let bundle = bundle_for(tag, sources)?;
        Ok(Catalog {
            bundles: HashMap::from([(tag.to_string(), bundle)]),
        })
    }

    fn bundle(&self, locale: &Locale) -> Option<&Bundle> {
        self.bundles
            .get(&locale.tag())
            .or_else(|| self.bundles.get(crate::locale::DEFAULT))
    }

    fn miss(id: &str) {
        let mut said = SAID.lock().unwrap_or_else(|p| p.into_inner());
        if said.insert(id.to_string()) {
            tracing::warn!(target: "bisa_i18n", id, "no message in the catalog; the id is shown");
        }
    }

    /// Whether `locale` has a message `id`.
    pub fn has(&self, locale: &Locale, id: &str) -> bool {
        self.bundle(locale).is_some_and(|b| b.has_message(id))
    }

    /// The sentence for `text`, or its id when the catalog has none.
    pub fn render(&self, locale: &Locale, text: &Text) -> String {
        let Some(bundle) = self.bundle(locale) else {
            Self::miss(&text.id);
            return text.id.to_string();
        };
        let Some(pattern) = bundle.get_message(&text.id).and_then(|m| m.value()) else {
            Self::miss(&text.id);
            return text.id.to_string();
        };
        let args = fluent_args(text);
        let mut errors = Vec::new();
        let out = bundle.format_pattern(pattern, Some(&args), &mut errors);
        if !errors.is_empty() {
            tracing::debug!(target: "bisa_i18n", id = %text.id, ?errors, "a message formatted with faults");
        }
        out.into_owned()
    }

    /// A catalog entry's display field in `locale` — the message `id`, or its
    /// `attribute` — when a translation ships, else the shipped file's own
    /// text, `fallback`. Content takes no arguments; a miss is no fault and
    /// is not logged: English ships no entries here at all.
    pub fn content(
        &self,
        locale: &Locale,
        id: &str,
        attribute: Option<&str>,
        fallback: &str,
    ) -> String {
        let Some(bundle) = self.bundle(locale) else {
            return fallback.to_string();
        };
        let Some(message) = bundle.get_message(id) else {
            return fallback.to_string();
        };
        let pattern = match attribute {
            Some(name) => message.get_attribute(name).map(|a| a.value()),
            None => message.value(),
        };
        let Some(pattern) = pattern else {
            return fallback.to_string();
        };
        let mut errors = Vec::new();
        bundle
            .format_pattern(pattern, None, &mut errors)
            .into_owned()
    }

    /// One attribute of message `id`, with `text`'s arguments.
    pub fn attribute(
        &self,
        locale: &Locale,
        id: &str,
        attribute: &str,
        text: &Text,
    ) -> Option<String> {
        let bundle = self.bundle(locale)?;
        let message = bundle.get_message(id)?;
        let attr = message.get_attribute(attribute)?;
        let args = fluent_args(text);
        let mut errors = Vec::new();
        Some(
            bundle
                .format_pattern(attr.value(), Some(&args), &mut errors)
                .into_owned(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn english() -> Locale {
        Locale::english()
    }

    #[test]
    fn a_language_not_shipped_has_no_source_and_a_bad_tag_or_file_is_a_fault() {
        assert!(Namespace::Settings.source("fr").is_none());
        let faults = Catalog::from_sources("not a tag!", &[])
            .err()
            .expect("a fault");
        assert!(
            faults[0].contains("not a language identifier"),
            "{faults:?}"
        );
        let faults = Catalog::from_sources("en", &["bad = {"])
            .err()
            .expect("a fault");
        assert!(faults.iter().all(|f| f.starts_with("en: ")), "{faults:?}");
        let faults = Catalog::from_sources("en", &["a = x", "a = y"])
            .err()
            .expect("a fault");
        assert!(
            faults.iter().any(|f| f.contains("a")),
            "a message said twice is a fault: {faults:?}"
        );
    }

    #[test]
    fn a_number_argument_rides_through_a_miss_is_said_once_and_a_fault_still_renders() {
        let c =
            Catalog::from_sources("en", &["units = { $n } units", "lost = { $missing }"]).unwrap();
        assert_eq!(
            c.render(&english(), &Text::new("units").arg("n", 1.5f64)),
            "1.5 units"
        );
        assert_eq!(
            c.render(&english(), &Text::new("units").arg("n", 2i64)),
            "2 units"
        );
        assert_eq!(
            c.render(&english(), &Text::new("nothing-here")),
            "nothing-here"
        );
        assert_eq!(
            c.render(&english(), &Text::new("nothing-here")),
            "nothing-here"
        );
        assert!(c.render(&english(), &Text::new("lost")).contains("missing"));
        // A catalog of another language alone: English has no bundle to fall
        // back on, so the id is shown.
        let fr = Catalog::from_sources("fr", &["units = { $n } unités"]).unwrap();
        assert_eq!(
            fr.render(&english(), &Text::new("units").arg("n", 1)),
            "units"
        );
        assert!(!fr.has(&english(), "units"));
    }

    #[test]
    fn a_content_field_is_the_translation_when_one_ships_else_the_fallback() {
        let c = Catalog::from_sources(
            "en",
            &["catalog-pet-cat = Chat\n    .description = Un chat\ncatalog-pet-dog =\n    .tagline = Woof\n"],
        )
        .unwrap();
        let en = english();
        assert_eq!(c.content(&en, "catalog-pet-cat", None, "Cat"), "Chat");
        assert_eq!(
            c.content(&en, "catalog-pet-cat", Some("description"), "A cat"),
            "Un chat"
        );
        assert_eq!(
            c.content(&en, "catalog-pet-cat", Some("tagline"), "Meow"),
            "Meow"
        );
        assert_eq!(
            c.content(&en, "catalog-pet-dog", None, "Dog"),
            "Dog",
            "no value, the file's text"
        );
        assert_eq!(
            c.content(&en, "catalog-pet-dog", Some("tagline"), "Bark"),
            "Woof"
        );
        assert_eq!(c.content(&en, "catalog-pet-fox", None, "Fox"), "Fox");
        let fr = Catalog::from_sources("fr", &["catalog-pet-cat = Chat"]).unwrap();
        assert_eq!(
            fr.content(&en, "catalog-pet-cat", None, "Cat"),
            "Cat",
            "no bundle, the file's text"
        );
    }
}
