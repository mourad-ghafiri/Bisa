/**
 * The native Edit menu's verbs as the webview hears them (ide/15 §Dispatch):
 * on macOS a menu key equivalent fires before the webview sees the key, so
 * the shell performs Undo · Redo · Cut · Copy · Paste · Select All natively
 * and then says the verb (`edit:verb`); the keymap replays the chord where
 * the focus is — a Files tree pastes, a field has already pasted and is left
 * alone. The ids and the words mirror `desktop/src-tauri/src/edit_menu.rs`.
 * Plain `.mjs`, so `node --test` reads it.
 */

/** The verbs, in the menu's order: the history above the OS's line, the clipboard below. */
export const EDIT_VERBS = Object.freeze(["undo", "redo", "cut", "copy", "paste", "select_all"]);

/**
 * The verbs a focused code editor is asked first: Select All has no DOM
 * event, and Undo and Redo have none either — the native `undo:` reaches
 * Monaco's hidden textarea, never the model's own history, and a synthetic
 * key undoes nothing.
 */
export const HANDED_TO_EDITOR = Object.freeze(["undo", "redo", "select_all"]);

/** Whether a verb is one the focused code editor is asked first. @param {string} verb */
export function handedToEditor(verb) {
  return HANDED_TO_EDITOR.includes(verb);
}

/**
 * Whether a verb the shell just said is replayed as its chord where the
 * focus is. Every verb is — except one handed to a code editor that took it:
 * Cut, Copy and Paste reach Monaco as DOM clipboard events; Select All, Undo
 * and Redo have none, so the editor is asked first (`ui/monaco`'s
 * `selectAllInFocusedEditor` and `undoInFocusedEditor`) and the chord is for
 * whoever else holds the focus — a Files tree selects its rows, a field keeps
 * the native selection or its native undo, the designer's canvas undoes its
 * step.
 * @param {"undo" | "redo" | "cut" | "copy" | "paste" | "select_all"} verb
 * @param {boolean} takenByEditor whether a focused code editor took the verb
 */
export function replaysChord(verb, takenByEditor) {
  return !(handedToEditor(verb) && takenByEditor);
}

/** The event the shell emits after a verb ran natively; its payload is the verb. */
export const EDIT_VERB_EVENT = "edit:verb";

// Read by `editMenuModel.test.mjs` and `shortcuts.test.mjs`: holds `src-tauri/src/edit_menu.rs`'s item ids to the verbs; the webview itself never builds one.
/** The menu item's id for a verb — the shell's `EditVerb::id`. @param {string} verb */
export function editVerbId(verb) {
  return `edit:${verb}`;
}

/** Whether a word is one of the verbs. @param {unknown} word */
export function isEditVerb(word) {
  return typeof word === "string" && EDIT_VERBS.includes(word);
}

/**
 * The key the verb's chord is spelt with — what the keymap's `files` chords
 * spell too (`Mod+X` · `Mod+C` · `Mod+V` · `Mod+A`), and Monaco's own ⌘Z.
 * @param {"undo" | "redo" | "cut" | "copy" | "paste" | "select_all"} verb
 * @returns {"x" | "c" | "v" | "a" | "z"}
 */
export function keyOfVerb(verb) {
  switch (verb) {
    case "undo":
    case "redo":
      return "z";
    case "cut":
      return "x";
    case "copy":
      return "c";
    case "paste":
      return "v";
    default:
      return "a";
  }
}

/** Whether the verb's chord holds Shift — ⌘⇧Z, redo's. @param {string} verb */
export function shiftOfVerb(verb) {
  return verb === "redo";
}
