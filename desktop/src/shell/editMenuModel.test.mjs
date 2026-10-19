/**
 * The Edit menu's verbs as the webview hears them. Run with
 * `node --test desktop/src/shell/editMenuModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { EDIT_VERBS, EDIT_VERB_EVENT, HANDED_TO_EDITOR, editVerbId, handedToEditor, isEditVerb, keyOfVerb, replaysChord, shiftOfVerb } from "./editMenuModel.mjs";

const shell = readFileSync(new URL("../../src-tauri/src/edit_menu.rs", import.meta.url), "utf8");

test("six verbs in the menu's order — the history above the line, the clipboard below — each with the OS's key, and no other word is one", () => {
  assert.deepEqual([...EDIT_VERBS], ["undo", "redo", "cut", "copy", "paste", "select_all"]);
  assert.deepEqual(EDIT_VERBS.map(keyOfVerb), ["z", "z", "x", "c", "v", "a"]);
  assert.deepEqual(EDIT_VERBS.filter(shiftOfVerb), ["redo"], "⌘⇧Z is redo's alone");
  for (const v of EDIT_VERBS) assert.ok(isEditVerb(v));
  assert.ok(!isEditVerb("quit"), "Quit is the application menu's predefined item, never a verb");
  assert.ok(!isEditVerb(null) && !isEditVerb(""));
});

test("the ids and the event mirror the shell's, word for word", () => {
  assert.equal(EDIT_VERB_EVENT, "edit:verb");
  assert.ok(shell.includes(`pub const EDIT_VERB_EVENT: &str = "${EDIT_VERB_EVENT}";`), "one event name on both sides");
  for (const v of EDIT_VERBS) {
    assert.equal(editVerbId(v), `edit:${v}`);
    assert.ok(shell.includes(`"${editVerbId(v)}"`), `the shell builds an item ${editVerbId(v)}`);
    assert.ok(shell.includes(`"${v}"`), `the shell says the word ${v}`);
  }
  assert.ok(shell.includes(`"CmdOrCtrl+V"`) && shell.includes(`"CmdOrCtrl+A"`), "the OS's key equivalents stay on the items");
  assert.ok(shell.includes(`"CmdOrCtrl+Z"`) && shell.includes(`"CmdOrCtrl+Shift+Z"`), "undo and redo keep theirs — Monaco's own, no keymap chord's");
});

test("Select All, Undo and Redo are handed to a focused code editor and replayed for everybody else; the clipboard verbs are always replayed", () => {
  assert.deepEqual([...HANDED_TO_EDITOR], ["undo", "redo", "select_all"]);
  for (const v of HANDED_TO_EDITOR) {
    assert.ok(handedToEditor(v));
    assert.equal(replaysChord(v, true), false, `${v}: the editor took it — no chord for anybody else to hear`);
    assert.equal(replaysChord(v, false), true, `${v}: no editor holds the focus — a Files tree selects its rows, the designer's canvas undoes its step`);
  }
  assert.ok(!handedToEditor("paste"));
  assert.equal(replaysChord("select_all", true), false, "the editor selected its whole text: no chord for the Files tree to hear");
  assert.equal(replaysChord("select_all", false), true, "no editor holds the focus: a Files tree selects its rows");
  for (const verb of ["cut", "copy", "paste"]) {
    assert.equal(replaysChord(verb, false), true, verb);
    assert.equal(replaysChord(verb, true), true, `${verb} reaches Monaco as a DOM clipboard event, and a Files tree by its chord`);
  }
});
