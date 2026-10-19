//! The Edit menu's six verbs — Undo · Redo · Cut · Copy · Paste · Select All
//! — as the shell drives them on macOS (ide/15 §Dispatch).
//!
//! A menu key equivalent fires before the webview ever sees the key: `NSMenu`
//! claims ⌘V and sends `paste:` down the responder chain. With muda's
//! *predefined* items that is the whole story — a field pastes, a `role=tree`
//! does nothing, and no `keydown` is produced, so the keymap's `paste_entry`
//! never fires and the Files tree pastes from its menu alone. So the four
//! verbs are **custom** items here, keeping their key equivalents, and the
//! shell does two things when one fires: it sends the native selector down
//! the responder chain itself — a field, Monaco, xterm perform the verb as
//! they always did, a tree does nothing — and it tells the webview
//! (`EDIT_VERB_EVENT`), whose keymap replays the chord where the focus is
//! (`shell/editMenu.ts`). Undo and Redo are verbs too: the predefined pair's
//! `undo:` reached only the hidden textarea's native history under Monaco,
//! never the model's, so the webview hands the two to the editor whose text
//! area holds the focus (`ui/monaco.undoInFocusedEditor`) and replays the
//! chord elsewhere — the designer's canvas undoes its step. Off macOS no
//! Edit menu is built at all — a menubar accelerator would only shadow the
//! keymap there, and WebView2 and WebKitGTK take Ctrl+C/V/X/A/Z in the webview.

use tauri::menu::{MenuItem, MenuItemBuilder};
use tauri::{AppHandle, Emitter, Manager, Runtime};

/// The event the webview hears after a verb ran natively: the verb's word.
pub const EDIT_VERB_EVENT: &str = "edit:verb";

/// One editing verb of the Edit menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditVerb {
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
}

impl EditVerb {
    /// Every verb, in the menu's order: the history above the OS's line,
    /// the clipboard from Cut below it.
    pub const ALL: [EditVerb; 6] = [
        EditVerb::Undo,
        EditVerb::Redo,
        EditVerb::Cut,
        EditVerb::Copy,
        EditVerb::Paste,
        EditVerb::SelectAll,
    ];

    /// The menu item's id — `edit:<word>`, the same string `shell/editMenuModel.mjs` spells.
    pub fn id(self) -> &'static str {
        match self {
            EditVerb::Undo => "edit:undo",
            EditVerb::Redo => "edit:redo",
            EditVerb::Cut => "edit:cut",
            EditVerb::Copy => "edit:copy",
            EditVerb::Paste => "edit:paste",
            EditVerb::SelectAll => "edit:select_all",
        }
    }

    /// The verb by its item id.
    pub fn from_id(id: &str) -> Option<EditVerb> {
        EditVerb::ALL.into_iter().find(|v| v.id() == id)
    }

    /// The word the webview hears — the event's payload.
    pub fn word(self) -> &'static str {
        match self {
            EditVerb::Undo => "undo",
            EditVerb::Redo => "redo",
            EditVerb::Cut => "cut",
            EditVerb::Copy => "copy",
            EditVerb::Paste => "paste",
            EditVerb::SelectAll => "select_all",
        }
    }

    /// The menu item's title.
    pub fn title(self) -> &'static str {
        match self {
            EditVerb::Undo => "Undo",
            EditVerb::Redo => "Redo",
            EditVerb::Cut => "Cut",
            EditVerb::Copy => "Copy",
            EditVerb::Paste => "Paste",
            EditVerb::SelectAll => "Select All",
        }
    }

    /// The key equivalent — the OS's own for each verb, which the keymap's
    /// `files` chords spell the same way (`Mod+X` · `Mod+C` · `Mod+V` · `Mod+A`);
    /// ⌘Z and ⌘⇧Z are Monaco's own and no keymap chord's.
    pub fn accelerator(self) -> &'static str {
        match self {
            EditVerb::Undo => "CmdOrCtrl+Z",
            EditVerb::Redo => "CmdOrCtrl+Shift+Z",
            EditVerb::Cut => "CmdOrCtrl+X",
            EditVerb::Copy => "CmdOrCtrl+C",
            EditVerb::Paste => "CmdOrCtrl+V",
            EditVerb::SelectAll => "CmdOrCtrl+A",
        }
    }

    // LCOV_EXCL_START the app's menu bar and its window; out by decision
    /// The standard editing action a responder performs for the verb.
    #[cfg(target_os = "macos")]
    pub fn selector(self) -> objc2::runtime::Sel {
        match self {
            EditVerb::Undo => objc2::sel!(undo:),
            EditVerb::Redo => objc2::sel!(redo:),
            EditVerb::Cut => objc2::sel!(cut:),
            EditVerb::Copy => objc2::sel!(copy:),
            EditVerb::Paste => objc2::sel!(paste:),
            EditVerb::SelectAll => objc2::sel!(selectAll:),
        }
    }
    // LCOV_EXCL_STOP
}

/// The six items, kept as app state so the webview's words can reach them
/// (`words.rs`, the `shell_words` command).
pub struct Verbs<R: Runtime>(Vec<(EditVerb, MenuItem<R>)>);

// LCOV_EXCL_START the app's menu bar and its window; out by decision
impl<R: Runtime> Verbs<R> {
    /// The verbs and their items in the menu's order — the menu draws its
    /// line before the first clipboard verb.
    pub fn entries(&self) -> impl Iterator<Item = (EditVerb, &MenuItem<R>)> {
        self.0.iter().map(|(verb, item)| (*verb, item))
    }

    /// The verbs in the webview's language; a word not given keeps its English.
    pub fn say(&self, words: &crate::words::ShellWords) {
        for (verb, item) in &self.0 {
            let word = match verb {
                EditVerb::Undo => &words.undo,
                EditVerb::Redo => &words.redo,
                EditVerb::Cut => &words.cut,
                EditVerb::Copy => &words.copy,
                EditVerb::Paste => &words.paste,
                EditVerb::SelectAll => &words.select_all,
            };
            if let Some(w) = word {
                if let Err(e) = item.set_text(w) {
                    tracing::warn!(target: "bisa_desktop", verb = verb.word(), "the Edit menu's word did not take: {e}");
                }
            }
        }
    }
}
// LCOV_EXCL_STOP

// LCOV_EXCL_START the app's menu bar and its window; out by decision
/// The six custom items, in the menu's order, each with its key equivalent.
pub fn items<R: Runtime, M: Manager<R>>(manager: &M) -> tauri::Result<Verbs<R>> {
    EditVerb::ALL
        .into_iter()
        .map(|verb| {
            MenuItemBuilder::with_id(verb.id(), verb.title())
                .accelerator(verb.accelerator())
                .build(manager)
                .map(|item| (verb, item))
        })
        .collect::<tauri::Result<Vec<_>>>()
        .map(Verbs)
}
// LCOV_EXCL_STOP

// LCOV_EXCL_START the app's menu bar and its window; out by decision
/// A verb's item fired: perform it natively where the focus can — the first
/// responder and its chain, a field or an editor's textarea; a tree does
/// nothing — then tell the webview, whose keymap replays the chord where the
/// focus is. Menu events arrive on the main thread; the marker guards it.
pub fn perform<R: Runtime>(app: &AppHandle<R>, verb: EditVerb) {
    #[cfg(target_os = "macos")]
    {
        use objc2::MainThreadMarker;
        use objc2_app_kit::NSApplication;
        match MainThreadMarker::new() {
            Some(mtm) => {
                // SAFETY: on the main thread, sending a standard editing action down
                // the key window's responder chain, with no target and no sender.
                let performed = unsafe {
                    NSApplication::sharedApplication(mtm).sendAction_to_from(
                        verb.selector(),
                        None,
                        None,
                    )
                };
                tracing::trace!(target: "bisa_desktop", verb = verb.word(), performed, "edit verb");
            }
            None => {
                tracing::warn!(target: "bisa_desktop", verb = verb.word(), "an edit verb fired off the main thread")
            }
        }
    }
    if let Err(e) = app.emit(EDIT_VERB_EVENT, verb.word()) {
        tracing::warn!(target: "bisa_desktop", "the edit verb did not reach the webview: {e}");
    }
}
// LCOV_EXCL_STOP

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_six_verbs_have_ids_that_round_trip_words_titles_and_the_os_key_equivalents() {
        assert_eq!(EditVerb::ALL.len(), 6);
        assert_eq!(
            &EditVerb::ALL[..3],
            [EditVerb::Undo, EditVerb::Redo, EditVerb::Cut],
            "the history above the menu's line, the clipboard from Cut below it — where `app_menu` draws the separator"
        );
        for verb in EditVerb::ALL {
            assert_eq!(EditVerb::from_id(verb.id()), Some(verb), "{}", verb.id());
            assert!(
                verb.id().starts_with("edit:"),
                "one namespace the webview's model mirrors"
            );
            assert_eq!(
                &verb.id()["edit:".len()..],
                verb.word(),
                "the id is the word under the namespace"
            );
            assert!(
                verb.accelerator().starts_with("CmdOrCtrl+"),
                "the OS's own chord"
            );
            assert!(!verb.title().is_empty());
        }
        assert_eq!(EditVerb::Paste.accelerator(), "CmdOrCtrl+V");
        assert_eq!(EditVerb::Undo.accelerator(), "CmdOrCtrl+Z");
        assert_eq!(EditVerb::Redo.accelerator(), "CmdOrCtrl+Shift+Z");
        assert_eq!(EditVerb::SelectAll.word(), "select_all");
        assert_eq!(EditVerb::from_id("edit:undo"), Some(EditVerb::Undo));
        assert_eq!(EditVerb::from_id("edit:redo"), Some(EditVerb::Redo));
        assert_eq!(
            EditVerb::from_id("edit:quit"),
            None,
            "Quit is the application menu's predefined item, never a verb here"
        );
        assert_eq!(EDIT_VERB_EVENT, "edit:verb");
    }
}
