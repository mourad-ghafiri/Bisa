/**
 * The webview's half of the Edit menu (ide/15 §Dispatch). On macOS the
 * menu's key equivalents — ⌘X ⌘C ⌘V ⌘A — fire before the webview sees the
 * key: the shell performs the verb natively down the responder chain and
 * then says it (`edit:verb`, `src-tauri/src/edit_menu.rs`). Here the chord
 * is **replayed** as a synthetic keydown on the element that has the focus,
 * so the one window listener (`shortcuts.ts`) resolves it with the real
 * contexts: inside a Files tree `paste_entry`, `copy_entry`, `cut_entry` or
 * `select_all_entries` fire; inside a field the typing guard leaves it, the
 * native verb having already run; in a shell no command spells it and
 * nothing happens. A synthetic event has no default action, so nothing
 * pastes twice.
 *
 * **Select All in a code editor is handed over, not replayed.** Monaco takes
 * Cut, Copy and Paste from the DOM's clipboard events, which the native verb
 * raises; Select All raises none, the native one lands on Monaco's hidden
 * input — a slice of the text — and a synthetic key carries no key code
 * Monaco reads. So the focused editor is asked first
 * (`ui/monaco.selectAllInFocusedEditor`) and the chord is replayed only when
 * no editor took it (`editMenuModel.replaysChord`).
 */

import { inDesktopShell } from "../api";
import { isMac } from "../ui/KeyHint";
import { selectAllInFocusedEditor } from "../ui/monaco";
import { EDIT_VERB_EVENT, isEditVerb, keyOfVerb, replaysChord } from "./editMenuModel.mjs";
import type { EditVerb } from "./editMenuModel.mjs";

/** The chord of a verb as a keydown, on the element that has the focus. */
function replayEditVerb(verb: EditVerb): void {
  if (!replaysChord(verb, verb === "select_all" && selectAllInFocusedEditor())) return;
  const target = document.activeElement ?? document.body;
  target.dispatchEvent(new KeyboardEvent("keydown", { key: keyOfVerb(verb), metaKey: isMac, ctrlKey: !isMac, bubbles: true, cancelable: true }));
}

/** Hear the shell's verbs; nothing outside the desktop shell. */
export function installEditMenu(): void {
  if (!inDesktopShell()) return;
  void import("@tauri-apps/api/event").then((ev) =>
    ev.listen<unknown>(EDIT_VERB_EVENT, (e) => {
      if (isEditVerb(e.payload)) replayEditVerb(e.payload);
    }),
  );
}
