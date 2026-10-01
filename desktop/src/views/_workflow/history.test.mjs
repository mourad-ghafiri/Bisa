/**
 * Undo and redo, tested where they live.
 *
 * Run with `npm test` from `desktop/`.
 */

import { strict as assert } from "node:assert";
import { test } from "node:test";

import { canRedo, canUndo, create, push, redo, undo } from "./history.mjs";

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
  assert.deepEqual(h.past, ["a", "b"]);
});

test("the same value is not a new edit", () => {
  const h = push(create("a"), "b");
  assert.equal(push(h, "b"), h);
});

test("the history is bounded", () => {
  let h = create(0, 3);
  for (let i = 1; i <= 10; i++) h = push(h, i);
  assert.equal(h.past.length, 3);
  assert.deepEqual(h.past, [7, 8, 9]);
});
