/**
 * What a terminal takes like a Mac terminal (ide/06 §The GPU, links and the
 * keyboard): the text-editing chords a line editor reads, typed through the
 * door typing takes, and a file pasted or dropped onto it typed as its path.
 * The rules are the models' (`typedKeysModel`, `pastedPathsModel`) and the
 * keymap's; these are source guards on the two files that wire them to
 * xterm, which no node test can mount.
 *
 * Run with `node --test desktop/src/scenarios/terminalInput.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { COMMANDS } from "../shell/keymapModel.mjs";
import { TYPED } from "../terminal/typedKeysModel.mjs";

const read = (rel) => readFileSync(new URL(rel, import.meta.url), "utf8");
const terminal = read("../terminal/Terminal.tsx");
const pathInput = read("../terminal/pathInput.ts");

/** The body of the custom key handler `Terminal.tsx` gives xterm. */
function keyHandler() {
  const at = terminal.indexOf("term.attachCustomKeyEventHandler(");
  assert.ok(at >= 0, "the terminal hands xterm a key handler");
  return terminal.slice(at, terminal.indexOf("\n  });", at));
}

test("a typed chord is typed before anything else may take it, through the door typing takes, and ends there", () => {
  const body = keyHandler();
  const typed = body.indexOf("typedFor(currentKeymap(), e, isMac)");
  assert.ok(typed >= 0, "the keymap decides which chord types what");
  assert.ok(typed < body.indexOf("refs.reserveKey.current"), "before the app's reserved chords, which would bubble it away");
  assert.match(body, /term\.input\(typed, true\)/, "typed as input — `onData`, the resume nudge's and the exit's guards — never written around them");
  assert.match(body, /e\.preventDefault\(\);\s*e\.stopPropagation\(\);/, "and the keystroke ends here, as one xterm handled would");
});

test("no ⌥+arrow is swallowed by hand: pane focus is the keymap's, rebindable, and ⌥←/⌥→ move a word on a Mac", () => {
  assert.doesNotMatch(terminal, /e\.altKey && \(e\.key === "Arrow/, "the hard-coded ⌥+arrow bubble is gone");
  const pane = COMMANDS.find((c) => c.id === "pane_left");
  assert.equal(pane?.mac?.default, "Mod+Alt+Left");
});

test("every typed command types something, and nothing else is typed", () => {
  assert.deepEqual(COMMANDS.filter((c) => c.typed).map((c) => c.id).sort(), Object.keys(TYPED).sort());
});

test("the mount takes pasted and dropped files as paths, and lets them go with the rest of its own", () => {
  assert.match(terminal, /live\.pathInput = attachPathInput\(host, term, \(\) => !live\.disposed\);/);
  assert.match(terminal, /live\.pathInput\?\.\(\);\s*live\.pathInput = null;/, "the teardown removes the listeners");
});

test("a paste asks the shell only when it carries files or no text, and every path goes through xterm's paste door", () => {
  assert.match(pathInput, /host\.addEventListener\("paste", onPaste, true\)/, "capture, ahead of xterm's own handler on its textarea");
  assert.match(pathInput, /if \(!pasteAsksShell\(\{ types: data\.types, text \}\)\) return;/, "plain text is left to xterm, untouched and undelayed");
  for (const door of ["pasteboardHolds()", "droppedPaths()", "pathsForDrop(names, paths)", "pasteImageToTemp(pastedImageName())"]) assert.ok(pathInput.includes(door), door);
  assert.ok(!/term\.write\(/.test(pathInput), "nothing is written to the screen around the shell");
  assert.equal((pathInput.match(/term\.paste\(/g) ?? []).length, 4, "paths, text, a saved picture's path and a drop — each through `term.paste`");
});
