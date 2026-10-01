//! One language for one process: the CLI's.
//!
//! The node renders per request (`Accept-Language`); a CLI process speaks to
//! one person, in one language, chosen once at start (`--lang`, else the
//! environment). Setting it here lets any line the CLI composes — in a pure
//! helper with no `Out` in reach — say a `Text` in that language with
//! [`say`]. English until set; set once, the first call wins.

use crate::{render, Locale};
use bisa_core::Text;
use std::sync::OnceLock;

static LOCALE: OnceLock<Locale> = OnceLock::new();

/// Choose the process's language. A second call changes nothing and says so.
pub fn set(locale: Locale) -> bool {
    LOCALE.set(locale).is_ok()
}

/// The process's language — English until [`set`].
pub fn current() -> Locale {
    LOCALE.get().cloned().unwrap_or_default()
}

/// A `Text` in the process's language.
pub fn say(text: &Text) -> String {
    render(&current(), text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_until_set_and_set_once() {
        assert_eq!(current().tag(), "en");
        assert_eq!(say(&bisa_core::text!("nothing-here")), "nothing-here");
        // The first choice stands; a test binary shares one process.
        let first = set(Locale::english());
        assert!(!set(Locale::english()) || first);
    }
}
