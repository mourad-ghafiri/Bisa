/**
 * Every chord lands (ide/15 §Dispatch), as the sources show it: every command
 * the keymap declares has a handler; every door `fire` opens has a listener
 * standing at it through `onDoor`, since a listener added by hand is one
 * `fire` never dispatches to; no native menu key equivalent shadows a declared
 * chord — a menu fires before the webview sees the key, which is how ⌘V in a
 * Files tree once did nothing; a terminal reserves keys by the scopes the
 * window listener reads; and the two places that once spelt a chord beside
 * the keymap resolve through it. Source assertions — no DOM.
 *
 * Run with `node --test desktop/src/shell/shortcuts.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

import { sourceFiles } from "../testWalk.mjs";
import { EDIT_VERBS, EDIT_VERB_EVENT, editVerbId } from "./editMenuModel.mjs";
import { COMMANDS, canonicalChord } from "./keymapModel.mjs";
import { TYPED } from "../terminal/typedKeysModel.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const src = join(here, "..");
const read = (rel) => readFileSync(join(src, rel), "utf8");
const shortcuts = read("shell/shortcuts.ts");
const shell = read("../src-tauri/src/main.rs");
const editMenuRs = read("../src-tauri/src/edit_menu.rs");

/** Commands handled by a component of their own, resolving through the keymap — each named with the file that proves it. */
const HANDLED_ELSEWHERE = Object.freeze({
  save: { file: "views/_workbench/EditorDoc.tsx", proof: '=== "save"' },
  cycle_conversation_mode: { file: "views/_workbench/useConversationPane.tsx", proof: '=== "cycle_conversation_mode"' },
});

test("every command the keymap declares is handled — by the window listener, the focused tree, the strip's nth tab, a focused terminal that types it, or a component that resolves through the keymap", () => {
  const explorer = read("ui/explorerStore.ts");
  const explorerIds = [...explorer.matchAll(/^\s+"([a-z_]+)",$/gm)].map((m) => m[1]);
  assert.ok(explorerIds.includes("paste_entry"), "the tree's verbs are read from explorerStore");
  const arms = new Set([...shortcuts.matchAll(/case "([a-z_0-9]+)":/g)].map((m) => m[1]));
  const terminal = read("terminal/Terminal.tsx");
  for (const { id, typed } of COMMANDS) {
    if (arms.has(id) || explorerIds.includes(id) || /^tab_[1-9]$/.test(id)) continue;
    // A typed command is the focused terminal's: it types the command's bytes, the chord resolved through the keymap.
    if (typed) {
      assert.ok(Object.prototype.hasOwnProperty.call(TYPED, id), `${id} is typed, and terminal/typedKeysModel.mjs has nothing to type for it`);
      assert.ok(terminal.includes("typedFor(currentKeymap(), e, isMac)"), `terminal/Terminal.tsx types ${id} through the keymap`);
      continue;
    }
    const elsewhere = HANDLED_ELSEWHERE[id];
    assert.ok(elsewhere, `${id} is declared and nothing handles it`);
    const text = read(elsewhere.file);
    assert.ok(text.includes(elsewhere.proof) && text.includes("commandForEvent("), `${elsewhere.file} resolves ${id} through the keymap`);
  }
  assert.ok(shortcuts.includes('case "close_browser_tab":'), "⌘W in a browser tab's body is the browser's");
});

test("every door fire opens has a listener standing at it through onDoor; the raw doors are raw on both sides", () => {
  const files = sourceFiles(src, (p) => /\.(ts|tsx)$/.test(p) && !p.includes("/scenarios/"));
  const texts = new Map(files.map((p) => [relative(src, p), readFileSync(p, "utf8")]));
  const fired = new Set();
  for (const text of texts.values()) for (const m of text.matchAll(/\bfire\(([A-Z_]+)[,)]/g)) fired.add(m[1]);
  assert.ok(fired.has("BROWSER_COMMAND") && fired.has("SELECT_TAB_AT"), "the firers are read");
  for (const door of fired) {
    const listening = [...texts.entries()].filter(([rel, text]) => rel !== "shell/shortcuts.ts" && text.includes("onDoor(") && new RegExp(`\\b${door}\\b`).test(text));
    assert.ok(listening.length > 0, `${door} is fired and nobody stands at it with onDoor — fire would pend it and drop it`);
  }
  assert.ok(read("shell/BrowserBar.tsx").includes("onDoor(BROWSER_COMMAND"), "the browser bar stands at its door");
  assert.ok(!read("shell/BrowserBar.tsx").includes("window.addEventListener(BROWSER_COMMAND"), "never a listener added by hand");
  for (const raw of ["DOC_FIND", "CONFLICT_COMMAND"]) {
    assert.ok(shortcuts.includes(`new CustomEvent(${raw}`), `${raw} is dispatched raw`);
    assert.ok(!fired.has(raw), `${raw} is not fired as a door`);
  }
});

test("no native menu key equivalent shadows a declared chord, and the Edit menu's verbs are the shell's own items", () => {
  /** The key equivalents muda's predefined items carry on macOS, by the builder method that adds them. */
  const PREDEFINED = { undo: "Mod+Z", redo: "Mod+Shift+Z", cut: "Mod+X", copy: "Mod+C", paste: "Mod+V", select_all: "Mod+A", minimize: "Mod+M", hide: "Mod+H", hide_others: "Mod+Alt+H", quit: "Mod+Q", fullscreen: "Ctrl+Mod+F", close_window: "Mod+W" };
  const menu = shell.slice(shell.indexOf("fn app_menu"), shell.indexOf("fn main()"));
  const used = Object.keys(PREDEFINED).filter((m) => menu.includes(`.${m}()`));
  assert.ok(used.includes("undo") && used.includes("quit"), "the menu is read");
  for (const verb of ["cut", "copy", "paste", "select_all", "close_window"]) {
    assert.ok(!used.includes(verb), `the predefined ${verb} item would claim its key before the webview sees it`);
  }
  const declared = new Map(COMMANDS.map((c) => [c.id, canonicalChord(c.chords.default)]));
  for (const m of used) {
    for (const [id, chord] of declared) assert.notEqual(chord, canonicalChord(PREDEFINED[m]), `${id} is shadowed by the menu's ${m}`);
  }
  assert.ok(menu.includes("edit_menu::items(handle)"), "the four verbs are custom items");
  assert.ok(shell.includes(".on_menu_event(") && shell.includes("edit_menu::perform(app, verb)"), "a verb runs natively and is told to the webview");
  assert.ok(shell.includes('#[cfg(target_os = "macos")]\nfn app_menu'), "the menu is macOS's alone");
  for (const v of EDIT_VERBS) assert.ok(editMenuRs.includes(`"${editVerbId(v)}"`), `the shell's item id for ${v}`);
  assert.ok(editMenuRs.includes(`"${EDIT_VERB_EVENT}"`), "one event name on both sides");
  const replay = read("shell/editMenu.ts");
  assert.ok(replay.includes(`new KeyboardEvent("keydown"`) && replay.includes("keyOfVerb(verb)"), "the verb is replayed as its chord where the focus is");
  assert.ok(read("main.tsx").includes("installEditMenu();"), "installed once with the shell");
});

test("a terminal reserves keys by the scopes the window listener reads, and no place spells a keymap chord beside the keymap", () => {
  const panel = read("shell/TerminalPanel.tsx");
  assert.ok(panel.includes("commandForEvent(km, e, contexts(e.target), isMac)"), "the same scopes as the window listener");
  assert.ok(!panel.includes('["workbench", "terminal"]'), "no literal context list");
  // Which scopes follow from where a key landed is `keyContextsModel`'s, tested there as behaviour — a document on any route among them.
  assert.ok(read("shell/keyContexts.ts").includes("scopesOf({"), "the scopes are the model's reading of the DOM");
  assert.ok(shortcuts.includes("const ctx = contexts(e.target);") && shortcuts.includes("commandForEvent(keymap, e, ctx, isMac)"), "the scopes are read once per key");
  const terminal = read("terminal/Terminal.tsx");
  assert.ok(terminal.includes('chordFor(currentKeymap(), "find")') && !terminal.includes('e.key === "f"'), "the terminal's find is the keymap's");
  const editor = read("views/_workbench/EditorDoc.tsx");
  assert.ok(editor.includes('=== "save"') && !editor.includes('e.key.toLowerCase() === "s"'), "the editor's save is the keymap's");
  const page = read("ui/artifact/pageInspector.mjs");
  assert.ok(page.includes("export function browserScript(relay, theme)") && page.includes("JSON.stringify(relay.map("), "the page relays the chords it is given");
  assert.ok(!page.includes('"[": "browser_back"'), "no chord table of its own");
  assert.ok(read("shell/BrowserPanel.tsx").includes("browserScript(relayChords(currentKeymap()), readInspectorTheme())"), "a tab opens with the keymap's chords");
});
