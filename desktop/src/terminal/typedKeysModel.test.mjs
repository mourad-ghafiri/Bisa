import test from "node:test";
import assert from "node:assert/strict";
import { COMMANDS, resolveKeymap } from "../shell/keymapModel.mjs";
import { TYPED, typedFor } from "./typedKeysModel.mjs";

const ev = (key, m = {}) => ({ key, metaKey: false, ctrlKey: false, shiftKey: false, altKey: false, ...m });

test("every typed command has its bytes, and every byte string is a typed command's", () => {
  const typed = COMMANDS.filter((c) => c.typed).map((c) => c.id);
  assert.deepEqual(Object.keys(TYPED).sort(), [...typed].sort());
  for (const id of typed) assert.equal(COMMANDS.find((c) => c.id === id)?.when, "terminal", `${id} is a terminal's`);
});

test("the bytes are readline's own keys — VS Code's on macOS, and what a harness's prompt reads", () => {
  assert.deepEqual(
    { ...TYPED },
    {
      line_start: "\u0001", // Ctrl+A
      line_end: "\u0005", // Ctrl+E
      word_left: "\u001bb", // Alt+B
      word_right: "\u001bf", // Alt+F
      delete_to_line_start: "\u0015", // Ctrl+U
      delete_word_right: "\u001bd", // Alt+D
    },
  );
  assert.ok(Object.isFrozen(TYPED));
});

test("a Mac terminal types each chord's key", () => {
  const km = resolveKeymap("default", null, true);
  assert.equal(typedFor(km, ev("ArrowLeft", { metaKey: true }), true), "\u0001", "⌘←");
  assert.equal(typedFor(km, ev("ArrowRight", { metaKey: true }), true), "\u0005", "⌘→");
  assert.equal(typedFor(km, ev("ArrowLeft", { altKey: true }), true), "\u001bb", "⌥←");
  assert.equal(typedFor(km, ev("ArrowRight", { altKey: true }), true), "\u001bf", "⌥→");
  assert.equal(typedFor(km, ev("Backspace", { metaKey: true }), true), "\u0015", "⌘⌫");
  assert.equal(typedFor(km, ev("Delete", { altKey: true }), true), "\u001bd", "⌥⌦");
});

test("nothing else is typed: other keys, ⌥⌫ (xterm's own), pane focus, and every key off a Mac", () => {
  const mac = resolveKeymap("default", null, true);
  assert.equal(typedFor(mac, ev("Backspace", { altKey: true }), true), null, "⌥⌫ stays xterm's Esc+Delete");
  assert.equal(typedFor(mac, ev("ArrowLeft", { altKey: true, metaKey: true }), true), null, "⌘⌥← is pane focus, the app's");
  assert.equal(typedFor(mac, ev("ArrowLeft"), true), null, "a bare arrow is the shell's");
  assert.equal(typedFor(mac, ev("ArrowLeft", { metaKey: true, shiftKey: true }), true), null, "⌘⇧← is not ⌘←");
  assert.equal(typedFor(mac, ev("a", { metaKey: true }), true), null);
  const rest = resolveKeymap("default", null, false);
  for (const e of [ev("ArrowLeft", { ctrlKey: true }), ev("ArrowLeft", { altKey: true }), ev("Home"), ev("Backspace", { ctrlKey: true })]) {
    assert.equal(typedFor(rest, e, false), null, `off a Mac ${JSON.stringify(e)} is xterm's own`);
  }
});

test("the keymap decides: a rebinding types on the new chord, an unbinding types nothing", () => {
  const moved = resolveKeymap("default", { line_start: "Ctrl+Shift+A" }, true);
  assert.equal(typedFor(moved, ev("ArrowLeft", { metaKey: true }), true), null, "the old chord no longer types");
  assert.equal(typedFor(moved, ev("A", { ctrlKey: true, shiftKey: true }), true), "\u0001");
  const off = resolveKeymap("default", { word_left: "" }, true);
  assert.equal(typedFor(off, ev("ArrowLeft", { altKey: true }), true), null);
  // A binding that is not typed, or not a terminal's, never types — whatever its id.
  assert.equal(typedFor({ bindings: [{ id: "line_start", when: "terminal", chord: "Mod+Left", typed: false }] }, ev("ArrowLeft", { metaKey: true }), true), null);
  assert.equal(typedFor({ bindings: [{ id: "line_start", when: "editor", chord: "Mod+Left", typed: true }] }, ev("ArrowLeft", { metaKey: true }), true), null);
});
