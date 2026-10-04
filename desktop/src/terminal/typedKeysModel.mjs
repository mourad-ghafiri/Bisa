/**
 * A Mac terminal's text editing, as the bytes it types (ide/06 §The GPU, links
 * and the keyboard). xterm sends nothing for ⌘+an arrow, and an escape
 * sequence for ⌥+an arrow that no shell's line editor reads by default; a Mac
 * terminal types the keys the line editor does read — readline's own, the ones
 * zsh and bash bind out of the box and a harness's prompt reads too (Claude
 * Code: Ctrl+A, Ctrl+E, Alt+B, Alt+F, Ctrl+U, Alt+D). The mapping is VS Code's
 * on macOS (`terminal.sendSequence.contribution.ts`): ⌘← Ctrl+A, ⌘→ Ctrl+E,
 * ⌘⌫ Ctrl+U, ⌥⌦ Alt+D — and ⌥←/⌥→ Alt+B/Alt+F, the word moves.
 *
 * Which chord types which is the keymap's (`shell/keymapModel.mjs`, the
 * `typed` commands), so a person rebinds or unbinds one in Settings › Keymap
 * like any other; this file holds only what each types. ⌥⌫ is not here: xterm
 * already types Esc+Delete for it, the previous word in a line editor and in a
 * harness's prompt alike. Plain `.mjs`, so `node --test` reads it.
 */

import { matchesEvent } from "../shell/keymapModel.mjs";

/** What each `typed` command types into the shell. */
export const TYPED = Object.freeze({
  /** Ctrl+A — the start of the line. */
  line_start: "\x01",
  /** Ctrl+E — the end of the line. */
  line_end: "\x05",
  /** Alt+B (Esc b) — back a word. */
  word_left: "\x1bb",
  /** Alt+F (Esc f) — forward a word. */
  word_right: "\x1bf",
  /** Ctrl+U — delete to the start of the line. */
  delete_to_line_start: "\x15",
  /** Alt+D (Esc d) — delete the next word. */
  delete_word_right: "\x1bd",
});

/**
 * What a key typed in a focused terminal types instead of what xterm would
 * send: the bytes of the `typed` terminal binding its chord is, or null — a
 * key that is not one, a command unbound, a binding of another scope.
 * @param {{bindings: readonly {id: string, when: string, chord: string | null, typed?: boolean}[]}} keymap
 * @param {{key: string, metaKey: boolean, ctrlKey: boolean, shiftKey: boolean, altKey: boolean}} e
 * @param {boolean} mac
 * @returns {string | null}
 */
export function typedFor(keymap, e, mac) {
  for (const b of keymap.bindings) {
    if (!b.typed || b.when !== "terminal" || !b.chord || !Object.prototype.hasOwnProperty.call(TYPED, b.id)) continue;
    if (matchesEvent(e, b.chord, mac)) return TYPED[b.id];
  }
  return null;
}
