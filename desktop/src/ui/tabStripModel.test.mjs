import { test } from "node:test";
import assert from "node:assert/strict";
import { nextStripIndex, stripTabStop } from "./tabStripModel.mjs";

test("the arrows walk the strip one tab at a time and never wrap", () => {
  assert.equal(nextStripIndex("ArrowRight", 0, 3), 1);
  assert.equal(nextStripIndex("ArrowRight", 2, 3), 2);
  assert.equal(nextStripIndex("ArrowLeft", 2, 3), 1);
  assert.equal(nextStripIndex("ArrowLeft", 0, 3), 0);
});

test("Home and End jump to the ends", () => {
  assert.equal(nextStripIndex("Home", 2, 5), 0);
  assert.equal(nextStripIndex("End", 0, 5), 4);
});

test("other keys, an empty strip and a tab not in the strip are not the strip's", () => {
  assert.equal(nextStripIndex("ArrowDown", 1, 3), null);
  assert.equal(nextStripIndex("Enter", 1, 3), null);
  assert.equal(nextStripIndex("ArrowRight", 0, 0), null);
  assert.equal(nextStripIndex("ArrowRight", -1, 3), null);
});

test("the one tab stop is the open tab, or the first when the open one is elsewhere", () => {
  assert.equal(stripTabStop(["a", "b", "c"], "b"), "b");
  assert.equal(stripTabStop(["a", "b", "c"], "z"), "a");
  assert.equal(stripTabStop(["a", "b"], null), "a");
  assert.equal(stripTabStop([], null), null);
});
