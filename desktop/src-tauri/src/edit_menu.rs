//! The Edit menu's four verbs — Cut · Copy · Paste · Select All — as the
//! shell drives them on macOS (ide/15 §Dispatch).
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
//! (`shell/editMenu.ts`). Undo and redo stay predefined: no keymap chord
//! spells ⌘Z. Off macOS no Edit menu is built at all — a menubar accelerator
//! would only shadow the keymap there, and WebView2 and WebKitGTK take
//! Ctrl+C/V/X/A in the webview.

use tauri::menu::{MenuItem, MenuItemBuilder};
use tauri::{AppHandle, Emitter, Manager, Runtime};

/// The event the webview hears after a verb ran natively: the verb's word.
pub const EDIT_VERB_EVENT: &str = "edit:verb";

/// One editing verb of the Edit menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditVerb {
    Cut,
    Copy,
    Paste,
    SelectAll,
}

impl EditVerb {
    /// Every verb, in the menu's order.
    pub const ALL: [EditVerb; 4] = [
        EditVerb::Cut,
        EditVerb::Copy,
        EditVerb::Paste,
        EditVerb::SelectAll,
    ];

    /// The menu item's id — `edit:<word>`, the same string `shell/editMenuModel.mjs` spells.
    pub fn id(self) -> &'static str {
        match self {
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
            EditVerb::Cut => "cut",
            EditVerb::Copy => "copy",
            EditVerb::Paste => "paste",
            EditVerb::SelectAll => "select_all",
        }
    }

    /// The menu item's title.
    pub fn title(self) -> &'static str {
        match self {
            EditVerb::Cut => "Cut",
            EditVerb::Copy => "Copy",
            EditVerb::Paste => "Paste",
            EditVerb::SelectAll => "Select All",
        }
    }

    /// The key equivalent — the OS's own for each verb, which the keymap's
    /// `files` chords spell the same way (`Mod+X` · `Mod+C` · `Mod+V` · `Mod+A`).
    pub fn accelerator(self) -> &'static str {
        match self {
            EditVerb::Cut => "CmdOrCtrl+X",
            EditVerb::Copy => "CmdOrCtrl+C",
            EditVerb::Paste => "CmdOrCtrl+V",
            EditVerb::SelectAll => "CmdOrCtrl+A",
        }
    }

    /// The standard editing action a responder performs for the verb.
    #[cfg(target_os = "macos")]
    pub fn selector(self) -> objc2::runtime::Sel {
        match self {
            EditVerb::Cut => objc2::sel!(cut:),
            EditVerb::Copy => objc2::sel!(copy:),
            EditVerb::Paste => objc2::sel!(paste:),
            EditVerb::SelectAll => objc2::sel!(selectAll:),
        }
    }
}

/// The four items, kept as app state so the webview's words can reach them
/// (`words.rs`, the `shell_words` command).
pub struct Verbs<R: Runtime>(Vec<(EditVerb, MenuItem<R>)>);

impl<R: Runtime> Verbs<R> {
    /// The items in the menu's order.
    pub fn items(&self) -> impl Iterator<Item = &MenuItem<R>> {
        self.0.iter().map(|(_, item)| item)
    }

    /// The verbs in the webview's language; a word not given keeps its English.
    pub fn say(&self, words: &crate::words::ShellWords) {
        for (verb, item) in &self.0 {
            let word = match verb {
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

/// The four custom items, in the menu's order, each with its key equivalent.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_four_verbs_have_ids_that_round_trip_words_titles_and_the_os_key_equivalents() {
        assert_eq!(EditVerb::ALL.len(), 4);
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
        assert_eq!(EditVerb::SelectAll.word(), "select_all");
        assert_eq!(
            EditVerb::from_id("edit:undo"),
            None,
            "undo is a predefined item, never a verb here"
        );
        assert_eq!(EDIT_VERB_EVENT, "edit:verb");
    }
}
