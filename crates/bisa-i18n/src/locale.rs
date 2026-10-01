//! Which language: a locale parsed, read from the environment or an
//! `Accept-Language` header, and negotiated against what the platform ships.

use fluent_langneg::{negotiate_languages, NegotiationStrategy};
use std::fmt;
use unic_langid::LanguageIdentifier;

/// The languages the platform ships a catalog for, best first. Adding a
/// language is a folder under `locales/` and a tag here (and in
/// `desktop/src/i18n/localeModel.mjs`, held equal by a guard).
pub const AVAILABLE: &[&str] = &["en"];

/// The language everything falls back to.
pub const DEFAULT: &str = "en";

/// A language the platform speaks — always one of [`AVAILABLE`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Locale(LanguageIdentifier);

impl Locale {
    /// The default: English.
    pub fn english() -> Locale {
        Locale(
            DEFAULT
                .parse()
                .expect("the default locale is a language identifier"),
        )
    }

    /// A tag as the person wrote it — `en`, `fr-CA`, `pt_BR`, `de_DE.UTF-8`
    /// — read as a language identifier; the encoding and the modifier a
    /// POSIX locale carries (`.UTF-8`, `@euro`) are not part of it.
    pub fn parse(tag: &str) -> Option<LanguageIdentifier> {
        let bare = tag.split(['.', '@']).next()?.trim().replace('_', "-");
        if bare.is_empty() || bare == "C" || bare == "POSIX" {
            return None;
        }
        bare.parse().ok()
    }

    /// The best shipped language for what was asked, in order of preference;
    /// nothing acceptable is the default.
    pub fn negotiate<S: AsRef<str>>(requested: &[S]) -> Locale {
        let wanted: Vec<LanguageIdentifier> = requested
            .iter()
            .filter_map(|t| Locale::parse(t.as_ref()))
            .collect();
        let available: Vec<LanguageIdentifier> = AVAILABLE
            .iter()
            .map(|t| t.parse().expect("AVAILABLE holds language identifiers"))
            .collect();
        let default = Locale::english().0;
        let chosen = negotiate_languages(
            &wanted,
            &available,
            Some(&default),
            NegotiationStrategy::Lookup,
        );
        Locale(chosen.first().map(|l| (*l).clone()).unwrap_or(default))
    }

    /// The environment's word, as POSIX has it: `LC_ALL`, then `LC_MESSAGES`,
    /// then `LANG`; the first that is set and not empty decides.
    pub fn from_env() -> Locale {
        let asked: Vec<String> = ["LC_ALL", "LC_MESSAGES", "LANG"]
            .iter()
            .filter_map(|k| std::env::var(k).ok())
            .filter(|v| !v.trim().is_empty())
            .take(1)
            .collect();
        Locale::negotiate(&asked)
    }

    /// An `Accept-Language` header's word: the tags in order of their
    /// quality, then negotiated.
    pub fn from_accept_language(header: &str) -> Locale {
        let mut tags: Vec<(f32, &str)> = header
            .split(',')
            .filter_map(|part| {
                let mut it = part.trim().split(';');
                let tag = it.next()?.trim();
                if tag.is_empty() || tag == "*" {
                    return None;
                }
                let q = it
                    .find_map(|p| p.trim().strip_prefix("q="))
                    .and_then(|q| q.trim().parse::<f32>().ok())
                    .unwrap_or(1.0);
                Some((q, tag))
            })
            .collect();
        // Stable: equal qualities keep the header's order.
        tags.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        let ordered: Vec<&str> = tags.into_iter().map(|(_, t)| t).collect();
        Locale::negotiate(&ordered)
    }

    /// The identifier, for a bundle.
    pub fn id(&self) -> &LanguageIdentifier {
        &self.0
    }

    /// The tag, for a header or a stamp: `en`.
    pub fn tag(&self) -> String {
        self.0.to_string()
    }
}

impl Default for Locale {
    fn default() -> Self {
        Locale::english()
    }
}

impl fmt::Display for Locale {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_posix_locale_is_read_without_its_encoding_and_c_is_nobody_s_language() {
        assert_eq!(Locale::parse("de_DE.UTF-8").unwrap().to_string(), "de-DE");
        assert_eq!(Locale::parse("fr_CA@euro").unwrap().to_string(), "fr-CA");
        assert_eq!(Locale::parse("en").unwrap().to_string(), "en");
        assert!(Locale::parse("C").is_none());
        assert!(Locale::parse("POSIX").is_none());
        assert!(Locale::parse("").is_none());
        assert!(Locale::parse("not a tag!").is_none());
    }

    #[test]
    fn negotiation_falls_back_to_english_and_keeps_the_asked_order() {
        assert_eq!(Locale::negotiate(&["fr", "en"]).tag(), "en");
        assert_eq!(
            Locale::negotiate(&["fr-FR"]).tag(),
            "en",
            "a language not shipped is English"
        );
        assert_eq!(Locale::negotiate::<&str>(&[]).tag(), "en");
        assert_eq!(
            Locale::negotiate(&["en-GB"]).tag(),
            "en",
            "a region of a shipped language is that language"
        );
    }

    #[test]
    fn an_accept_language_header_is_ordered_by_quality() {
        assert_eq!(
            Locale::from_accept_language("fr;q=0.5, en;q=0.9").tag(),
            "en"
        );
        assert_eq!(Locale::from_accept_language("*").tag(), "en");
        assert_eq!(Locale::from_accept_language("").tag(), "en");
        assert_eq!(Locale::from_accept_language("en-US,en;q=0.8").tag(), "en");
    }
}
