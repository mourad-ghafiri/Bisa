/**
 * Undo and redo, tested where they live.
 *
 * Run with `npm test` from `desktop/`.
 */

import { strict as assert } from "node:assert";
import { test } from "node:test";

import { COALESCE_MS, canRedo, canUndo, create, push, redo, undo } from "./history.mjs";

const values = (entries) => entries.map((e) => e.value);

test("push, undo and redo walk the list", () => {
  let h = create("a");
  assert.equal(canUndo(h), false);
  h = push(h, "b");
  h = push(h, "c");
  assert.equal(h.present, "c");
  h = undo(h);
  assert.equal(h.present, "b");
  assert.equal(canRedo(h), true);
  h = redo(h);
  assert.equal(h.present, "c");
  assert.equal(undo(undo(undo(h))).present, "a", "undo past the start stays at the start");
  assert.equal(redo(h), h, "redo with no future is a no-op");
});

test("a new edit after an undo drops the future", () => {
  let h = push(push(create("a"), "b"), "c");
  h = undo(h);
  h = push(h, "d");
  assert.equal(canRedo(h), false);
  assert.deepEqual(values(h.past), ["a", "b"]);
});

test("the same value is not a new edit", () => {
  const h = push(create("a"), "b");
  assert.equal(push(h, "b"), h);
});

test("the history is bounded", () => {
  let h = create(0, 3);
  for (let i = 1; i <= 10; i++) h = push(h, i);
  assert.equal(h.past.length, 3);
  assert.deepEqual(values(h.past), [7, 8, 9]);
});

test("a typed burst is one step: pushes with one token inside the window replace the head", () => {
  const typed = (h, v, at) => push(h, v, { token: "step:build", at, selection: "build" });
  let h = create("a");
  h = typed(h, "ab", 1000);
  h = typed(h, "abc", 1300);
  h = typed(h, "abcd", 1300 + COALESCE_MS);
  assert.deepEqual([values(h.past), h.present], [["a"], "abcd"], "three keystrokes, one entry");
  assert.equal(undo(h).present, "a", "undo takes the whole word back");
  // The window is measured from the last keystroke, and a pause cuts it.
  h = typed(h, "abcde", 1300 + COALESCE_MS * 2 + 1);
  assert.deepEqual(values(h.past), ["a", "abcd"]);
  // Another step's field is another burst.
  h = push(h, "abcde!", { token: "step:ship", at: h.at + 10, selection: "ship" });
  assert.deepEqual(values(h.past), ["a", "abcd", "abcde"]);
  // A structural edit carries no token and never merges — before or after a burst.
  h = push(h, "moved", { at: h.at + 10, selection: "ship" });
  h = push(h, "moved!", { token: "step:ship", at: h.at + 10, selection: "ship" });
  assert.deepEqual(values(h.past), ["a", "abcd", "abcde", "abcde!", "moved"]);
  assert.equal(push(create("x"), "y", { token: "k", at: 5 }).at, 5, "the head keeps when it was pushed");
  const back = push(push(create("x"), "y", { token: "k", at: 50 }), "z", { token: "k", at: 10 });
  assert.deepEqual(values(back.past), ["x", "y"], "a clock that went backwards never merges");
});

test("every entry carries the selection it was left with, and undo and redo hand it back", () => {
  let h = create("a");
  h = push(h, "b", { selection: { kind: "step", id: "build" } });
  h = push(h, "c", { selection: { kind: "step", id: "ship" } });
  assert.deepEqual(h.past.map((e) => e.selection), [{ kind: "step", id: "build" }, { kind: "step", id: "ship" }]);
  assert.equal(h.landed, null, "a push lands nowhere");
  h = undo(h, { kind: "flow", from: "a", to: "b" });
  assert.deepEqual([h.present, h.landed], ["b", { kind: "step", id: "ship" }], "undo lands on the selection the entry was left with");
  assert.deepEqual(h.future[0], { value: "c", selection: { kind: "flow", from: "a", to: "b" } }, "and stamps the head it left with where the person was");
  h = redo(h, null);
  assert.deepEqual([h.present, h.landed], ["c", { kind: "flow", from: "a", to: "b" }], "redo lands where the undo left");
  assert.deepEqual(h.past[h.past.length - 1], { value: "b", selection: null });
  assert.equal(undo(create("x")).landed, null, "nothing to undo lands nowhere");
  // A restored head never coalesces: the next typed push is its own entry.
  h = push(undo(push(create("a"), "ab", { token: "k", at: 1 })), "ac", { token: "k", at: 2 });
  assert.deepEqual(values(h.past), ["a"]);
  assert.equal(push(h, "ad", { token: "k", at: 3 }).landed, null, "a push after a landing clears it");
});
