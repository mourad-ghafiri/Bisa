//! The words the shell's own menus say — the menu bar icon's fixed lines,
//! the Edit menu's verbs — in the language the webview speaks
//! (17 — Internationalisation). The shell holds no catalog: it builds its
//! menus with English, and the webview pushes the words once it has said
//! its own (`shell_words`); a word not given keeps its English.

use serde::Deserialize;

/// The words, each optional: the webview sends the ones it has.
#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct ShellWords {
    /// *Open Bisa* — the menu bar icon's line that shows the window.
    pub open: Option<String>,
    /// *Show in Dock* — the menu bar icon's Dock switch.
    pub dock: Option<String>,
    /// *Quit Bisa* — the menu bar icon's last line.
    pub quit: Option<String>,
    /// The Edit menu's four verbs.
    pub cut: Option<String>,
    pub copy: Option<String>,
    pub paste: Option<String>,
    pub select_all: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_word_not_sent_is_none_so_the_english_stays() {
        let words: ShellWords =
            serde_json::from_str("{}").expect("an empty object is every word unsaid");
        assert_eq!(words, ShellWords::default());
        let words: ShellWords =
            serde_json::from_str(r#"{"open":"Ouvrir Bisa","select_all":"Tout sélectionner"}"#)
                .expect("two words");
        assert_eq!(words.open.as_deref(), Some("Ouvrir Bisa"));
        assert_eq!(words.select_all.as_deref(), Some("Tout sélectionner"));
        assert_eq!(words.quit, None);
    }
}
