/**
 * Undo and redo in the Project IDE's editor (ide/03 §Editing features, ide/15
 * §Dispatch): on macOS ⌘Z and ⌘⇧Z are the Edit menu's own items, handed to
 * the Monaco editor whose text area holds the focus and replayed as their
 * chord for everybody else; the model is the tab's, kept across every unmount
 * and let go when the tab closes; a value pushed from outside is one undoable
 * step. The models are driven; the wiring is read off the sources, no DOM.
 *
 * Run with `node --test --import ./src/i18n/preload.mjs src/scenarios/editorUndo.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { KeptModels } from "../ui/monacoModelsModel.mjs";
import { EDIT_VERBS, HANDED_TO_EDITOR, editVerbId, keyOfVerb, replaysChord, shiftOfVerb } from "../shell/editMenuModel.mjs";
import { COMMANDS, canonicalChord, chordFor, resolveKeymap } from "../shell/keymapModel.mjs";
import { closeTab, closedTabs, emptyWorkbench, openTab, retargetTabs, tabId } from "../views/_workbench/workbenchModel.mjs";

const src = (rel) => readFileSync(new URL(rel, import.meta.url), "utf8");
const between = (text, from, to) => text.slice(text.indexOf(from), text.indexOf(to, text.indexOf(from)));
const inOrder = (text, needles, why) => {
  let at = -1;
  for (const n of needles) {
    const next = text.indexOf(n, at + 1);
    assert.ok(next > at, `${why}: ${JSON.stringify(n)} comes after what precedes it`);
    at = next;
  }
};

test("the Edit menu's ⌘Z and ⌘⇧Z are the shell's own items, not muda's predefined ones — the history above the line, the clipboard below", () => {
  const main = src("../../src-tauri/src/main.rs");
  const menu = between(main, "fn app_menu", "fn main()");
  assert.ok(!menu.includes(".undo()") && !menu.includes(".redo()"), "a predefined item would claim the key before the webview sees it");
  assert.ok(menu.includes("edit_menu::items(handle)") && menu.includes("verbs.entries()"), "the six verbs are the shell's items");
  inOrder(menu, ["if verb == edit_menu::EditVerb::Cut", "edit.separator()", "edit = edit.item(item)"], "the OS's line is drawn before the first clipboard verb");
  const shell = src("../../src-tauri/src/edit_menu.rs");
  const all = between(shell, "pub const ALL", "];");
  assert.ok(all.indexOf("EditVerb::Undo") < all.indexOf("EditVerb::Redo") && all.indexOf("EditVerb::Redo") < all.indexOf("EditVerb::Cut"), "Undo, Redo, then the clipboard");
  assert.ok(shell.includes('"CmdOrCtrl+Z"') && shell.includes('"CmdOrCtrl+Shift+Z"'), "the key equivalents");
  assert.ok(shell.includes("objc2::sel!(undo:)") && shell.includes("objc2::sel!(redo:)"), "the native selector still runs for every field");
  assert.deepEqual([...EDIT_VERBS], ["undo", "redo", "cut", "copy", "paste", "select_all"]);
  for (const v of EDIT_VERBS) assert.ok(shell.includes(`"${editVerbId(v)}"`), `the shell builds ${editVerbId(v)}`);
});

test("the webview hands undo and redo to the Monaco editor with text focus — its own history — and replays the chord for the designer's canvas", () => {
  const monaco = src("../ui/monaco.ts");
  const handOver = between(monaco, 'export function undoInFocusedEditor(verb: "undo" | "redo"): boolean', "\n}\n");
  assert.ok(handOver.includes("editor.hasTextFocus()") && handOver.includes('editor.trigger("menu", verb, null)'), "text focus, then Monaco's own undo command");
  assert.ok(!handOver.includes("getDomNode()?.contains"), "never widget focus: a find widget's input undoes natively on its own");
  const menu = src("../shell/editMenu.ts");
  assert.ok(menu.includes("return undoInFocusedEditor(verb);") && menu.includes("replaysChord(verb, takenByEditor(verb))"), "handed over first, replayed only when no editor took it");
  assert.ok(menu.includes("shiftKey: shiftOfVerb(verb)"), "the replayed chord carries ⇧ for redo");
  assert.deepEqual(EDIT_VERBS.map(keyOfVerb).slice(0, 2), ["z", "z"]);
  assert.deepEqual(EDIT_VERBS.filter(shiftOfVerb), ["redo"]);
  for (const v of HANDED_TO_EDITOR) assert.equal(replaysChord(v, true), false, v);
  assert.equal(replaysChord("undo", false), true, "no editor holds the focus: the designer's canvas hears its chord");
  const canvas = src("../ui/FlowCanvas.tsx");
  assert.ok(canvas.includes('e.key.toLowerCase() === "z"') && canvas.includes("if (e.shiftKey) onRedo?.();"), "the canvas reads the replayed key and its shift");
});

test("undo and redo are the Edit menu's verbs, never keymap commands, and the shell says their words in the window's language", () => {
  assert.ok(COMMANDS.every(({ id }) => id !== "undo" && id !== "redo"), "on macOS the key never reaches the webview, elsewhere Monaco binds it itself — a keymap command would fire never or twice");
  const onMac = resolveKeymap("default", null, true);
  for (const { id } of COMMANDS) {
    const chord = chordFor(onMac, id);
    if (chord) assert.ok(chord !== canonicalChord("Mod+Z") && chord !== canonicalChord("Mod+Shift+Z"), `${id} would be shadowed by the menu's item`);
  }
  const words = src("../../src-tauri/src/words.rs");
  assert.ok(words.includes("pub undo: Option<String>") && words.includes("pub redo: Option<String>"), "the shell takes the two words");
  const sender = src("../shell/shellWords.ts");
  assert.ok(sender.includes('undo: t("shell-words-undo")') && sender.includes('redo: t("shell-words-redo")'), "the webview says them");
});

test("the model is the tab's: acquired at mount, left at unmount, let go when the tab closes; a pushed value is one undoable step, never setValue", () => {
  const editor = src("../ui/CodeEditor.tsx");
  assert.ok(editor.includes("keep ? acquireModel(m, keep, value, lang, path) : m.editor.createModel(value, lang, freeFileUri(m, path))"), "the tab's model when there is a tab to keep it for");
  assert.ok(editor.includes("if (!keep) model.dispose();"), "a kept model is never disposed by an unmount");
  assert.ok(editor.includes("}, [path, language, plain, keep]);"), "a new tab is a new mount");
  assert.ok(!editor.includes("model.setValue("), "setValue destroys the history and flushes the caret");
  assert.ok(editor.includes("pushValue(ed, model, value, pushing);") && editor.includes("pushValue(ed, model, valueRef.current, pushing);"), "the value effect and the mount both push");
  inOrder(editor, ["model.onDidChangeContent(() => {", "if (pushing.current) return;", "onChangeRef.current?.(model.getValue());"], "a push from outside is not echoed as a keystroke");
  const models = src("../ui/monacoModels.ts");
  inOrder(models, ["model.pushStackElement();", "model.pushEditOperations(ed.getSelections(), [{ range: model.getFullModelRange(), text: value }], () => null);", "model.pushStackElement();"], "one undoable step between two undo stops");
  assert.ok(models.includes("if (model.getValue() === value) return;") && models.includes("ed.saveViewState()") && models.includes("ed.restoreViewState(view)"), "an equal text is no step; the view is kept through the push");
  assert.ok(models.includes("m.editor.setModelLanguage(model, language)") && models.includes("m.editor.getModel(uri) ? undefined : uri"), "a kept model is relabelled, and a second view of a file gets no address");
  const diff = src("../ui/DiffEditor.tsx");
  assert.ok(diff.includes("keep ? acquireModel(m, keep, modified, language, path) :") && diff.includes("if (!keep) modifiedModel.dispose();"), "the modified side is the tab's live buffer");
  assert.ok(diff.includes("om.setValue(original)") && !diff.includes("mm.setValue("), "the original side is nobody's history; the modified side is pushed");
  const doc = src("../views/_workbench/EditorDoc.tsx");
  assert.equal((doc.match(/keep=\{registryKey\}/g) ?? []).length, 4, "the code editor and the three diff views — the review lens, the conflict review, the HEAD diff — keep the tab's model");
  const store = src("../views/_workbench/workbenchStore.ts");
  inOrder(store, ["forgetBuffers(gone);", "forgetViews(gone);", "releaseModels(gone);"], "a closed tab's model goes with its buffer and its place");
  assert.ok(!src("../views/_work/ConflictBlock.tsx").includes("keep="), "the Git panel's conflict editor keeps a model of its own");
});

test("a tab switch keeps the model, a close lets it go, and a rename on disk starts a new one under the new key", () => {
  const ROOT = "ws:w1";
  const keyOf = (tab) => `${ROOT}|${tabId(tab)}`;
  const kept = new KeptModels();
  const log = [];
  const model = (name) => ({ name, dispose: () => log.push(name) });
  const a = { kind: "file", path: "src/a.ts" };
  const b = { kind: "file", path: "src/b.ts" };
  let tabs = openTab(emptyWorkbench(), ROOT, a);
  const first = kept.acquire(keyOf(a), () => model("a"));
  // Another file opens on top: a's editor unmounts, its model stays.
  tabs = openTab(tabs, ROOT, b);
  kept.acquire(keyOf(b), () => model("b"));
  assert.equal(kept.count, 2);
  // Back to a: the same model, its history with it.
  assert.equal(kept.acquire(keyOf(a), () => model("a-again")), first);
  assert.deepEqual(log, []);
  // b closes: the one place tabs change lets its model go.
  const after = closeTab(tabs, ROOT, tabId(b));
  const gone = closedTabs(tabs, after).map(([root, id]) => `${root}|${id}`);
  assert.deepEqual(gone, [keyOf(b)]);
  assert.equal(kept.release(gone), 1);
  assert.deepEqual(log, ["b"]);
  tabs = after;
  // a is renamed on disk: the tab follows, the old key is released with it, the new key starts fresh.
  const moved = retargetTabs(tabs, ROOT, "src/a.ts", "src/a2.ts");
  assert.deepEqual(moved.moved, [["src/a.ts", "src/a2.ts"]]);
  const oldKeys = closedTabs(tabs, moved.state).map(([root, id]) => `${root}|${id}`);
  assert.deepEqual(oldKeys, [keyOf(a)]);
  assert.equal(kept.release(oldKeys), 1);
  const renamed = kept.acquire(keyOf({ kind: "file", path: "src/a2.ts" }), () => model("a2"));
  assert.equal(renamed.name, "a2", "a model's URI is the language server's address and cannot change: the history starts over");
  assert.equal(kept.count, 1);
});
