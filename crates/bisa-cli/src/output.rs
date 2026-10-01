//! Output discipline: `--json` prints exactly one JSON value on stdout;
//! human mode prints prose — in the person's language. Activity/progress
//! lines always go to stderr.
//!
//! The language is the environment's (`LC_ALL` · `LC_MESSAGES` · `LANG`) or
//! `--lang`'s, negotiated against what the platform ships
//! (17 — Internationalisation). A sentence the CLI says is a `Text` of the
//! catalog (`locales/<lang>/cli.ftl`), rendered here through `say`; a
//! value a person typed — a title, a path — is an argument, shown as it is.
//! `human` remains for a line that is not a sentence: a table row of ids
//! and values, a path, a URL.

use bisa_core::Text;
use bisa_i18n::Locale;

/// A yes/no question on stderr, answered on stdin: `y`, `Y` or `yes` is yes,
/// anything else — an empty line, a closed stdin, a prompt that could not be
/// written — is no. The one confirmation the CLI asks, so `run` and `project`
/// ask it the same way.
pub fn confirm_on_stderr(question: &str) -> bool {
    use std::io::Write as _;
    eprint!("{question}");
    if std::io::stderr().flush().is_err() {
        return false;
    }
    let mut line = String::new();
    if std::io::stdin().read_line(&mut line).is_err() {
        return false;
    }
    matches!(line.trim(), "y" | "Y" | "yes")
}

pub struct Out {
    json: bool,
    locale: Locale,
}

impl Out {
    /// The one `Out` of a run; the language becomes the process's as well
    /// (`bisa_i18n::process`), so a line composed far from here says it too.
    pub fn new(json: bool, locale: Locale) -> Self {
        bisa_i18n::process::set(locale.clone());
        Self { json, locale }
    }

    /// A `Text` rendered in this language — for a line composed of several.
    pub fn text(&self, text: &Text) -> String {
        bisa_i18n::render(&self.locale, text)
    }

    /// One attribute of a message — a setting's `.help` — or nothing.
    pub fn attribute(&self, id: &str, attribute: &str) -> Option<String> {
        bisa_i18n::attribute(&self.locale, id, attribute, &Text::new(id.to_string()))
    }

    /// A sentence of the catalog, said to a person (stdout, suppressed in --json mode).
    pub fn say(&self, text: &Text) {
        self.human(&self.text(text));
    }

    /// Human-facing result text (stdout, suppressed in --json mode).
    pub fn human(&self, text: &str) {
        if !self.json {
            println!("{text}");
        }
    }

    /// The command's JSON result (stdout, only in --json mode).
    pub fn json_value(&self, value: serde_json::Value) {
        if self.json {
            println!("{value}");
        }
    }

    pub fn error(&self, text: &str) {
        if self.json {
            eprintln!("{}", serde_json::json!({"error": text}));
        } else {
            eprintln!(
                "{}",
                bisa_i18n::render(
                    &self.locale,
                    &bisa_core::text!("cli-output-error", text = text.to_string())
                )
            );
        }
    }
}
