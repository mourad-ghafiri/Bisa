/**
 * A rail of icon tabs: one press rule, one focus rule, one tab stop. Run with
 * `node --test desktop/src/ui/iconRailModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { nextRailIndex, pressRailTab, railAnchor } from "./iconRailModel.mjs";

test("a press opens, switches, or closes — the rail stays", () => {
  assert.deepEqual(pressRailTab({ open: false, tab: "a" }, "b"), { open: true, tab: "b" }, "closed: open on it");
  assert.deepEqual(pressRailTab({ open: true, tab: "a" }, "b"), { open: true, tab: "b" }, "open elsewhere: switch");
  assert.deepEqual(pressRailTab({ open: true, tab: "b" }, "b"), { open: false, tab: "b" }, "open on it: close, remembering it");
  assert.deepEqual(pressRailTab({ open: false, tab: "b" }, "b"), { open: true, tab: "b" }, "closed on it: open it again");
});

test("the keys move focus without wrapping; a key the rail does not take is nobody's", () => {
  assert.equal(nextRailIndex("ArrowDown", 0, 3), 1);
  assert.equal(nextRailIndex("ArrowDown", 2, 3), 2, "no wrap at the end");
  assert.equal(nextRailIndex("ArrowUp", 0, 3), 0, "no wrap at the start");
  assert.equal(nextRailIndex("ArrowUp", 2, 3), 1);
  assert.equal(nextRailIndex("Home", 2, 3), 0);
  assert.equal(nextRailIndex("End", 0, 3), 2);
  assert.equal(nextRailIndex("ArrowDown", -1, 3), 0, "off the rail, down lands on the first");
  assert.equal(nextRailIndex("ArrowUp", -1, 3), 2, "off the rail, up lands on the last");
  assert.equal(nextRailIndex("ArrowLeft", 1, 3), null);
  assert.equal(nextRailIndex("Enter", 1, 3), null);
  assert.equal(nextRailIndex("ArrowDown", 0, 0), null, "an empty rail has nowhere to go");
});

test("the rail keeps one tab stop: the showing tab while open, else the first", () => {
  assert.equal(railAnchor(["a", "b"], { open: true, tab: "b" }), "b");
  assert.equal(railAnchor(["a", "b"], { open: false, tab: "b" }), "a", "a closed column marks nothing; the first is reachable");
  assert.equal(railAnchor(["a", "b"], { open: true, tab: "zzz" }), "a", "a tab the rail lacks anchors the first");
  assert.equal(railAnchor([], { open: true, tab: "a" }), null);
});
