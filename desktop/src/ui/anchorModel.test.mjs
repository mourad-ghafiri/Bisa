/**
 * A list's place kept as a row. Run with
 * `node --test desktop/src/ui/anchorModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { anchorOf, firstVisible, parseAnchor, scrollFor } from "./anchorModel.mjs";

/** Rows of 36 pixels from `from`, one per key. */
const rows = (keys, from = 0, height = 36) => keys.map((key, i) => ({ key, top: from + i * height, height }));

test("the first row whose foot is below the top edge is the anchor", () => {
  const list = rows(["a", "b", "c", "d"]);
  assert.equal(firstVisible(list, 0), 0);
  assert.equal(firstVisible(list, 35), 0, "a pixel of the first row is still in view");
  assert.equal(firstVisible(list, 36), 1, "a row whose foot is on the edge is above it");
  assert.equal(firstVisible(list, 100), 2);
  assert.deepEqual(anchorOf(list, 100), { key: "c", offset: 28 });
  assert.deepEqual(anchorOf(list, 72), { key: "c", offset: 0 });
  assert.equal(firstVisible(list, 144), -1, "a viewport below every row sees none");
  assert.equal(anchorOf(list, 144), null);
  assert.equal(firstVisible([], 10), -1);
  // Rows that size themselves: the heights are what they are.
  const measured = [{ key: "a", top: 0, height: 36 }, { key: "b", top: 36, height: 240 }, { key: "c", top: 276, height: 36 }];
  assert.deepEqual(anchorOf(measured, 200), { key: "b", offset: 164 }, "deep in an open row");
});

test("a list at its origin keeps no place", () => {
  assert.equal(anchorOf(rows(["a", "b"]), 0), null);
  assert.equal(anchorOf(rows(["a", "b"]), -4), null, "an overscroll is the origin");
  assert.equal(anchorOf(rows(["a", "b"]), Number.NaN), null);
});

test("a viewport above the first row anchors on it with a negative offset", () => {
  // The rows handed over are the window drawn, which begins below the edge.
  const drawn = rows(["k", "l", "m"], 360);
  assert.equal(firstVisible(drawn, 300), 0);
  assert.deepEqual(anchorOf(drawn, 300), { key: "k", offset: -60 });
  assert.equal(scrollFor({ key: "k", offset: -60 }, drawn), 300, "and is put back above it");
});

test("an anchor is put back on its row, wherever the row is now", () => {
  const anchor = anchorOf(rows(["a", "b", "c", "d"]), 100);
  // Two rows landed at the head: the row is 72 pixels further down.
  assert.equal(scrollFor(anchor, rows(["y", "z", "a", "b", "c", "d"])), 172);
  // The rows above it were measured taller than they were guessed.
  assert.equal(scrollFor(anchor, [{ key: "a", top: 0, height: 90 }, { key: "b", top: 90, height: 36 }, { key: "c", top: 126, height: 36 }]), 154);
  assert.equal(scrollFor({ key: "a", offset: -20 }, rows(["a"])), 0, "never above the origin");
});

test("an anchor whose row is gone is no place", () => {
  assert.equal(scrollFor({ key: "c", offset: 28 }, rows(["a", "b", "d"])), null);
  assert.equal(scrollFor({ key: "c", offset: 28 }, []), null);
  assert.equal(scrollFor(null, rows(["a"])), null);
  assert.equal(scrollFor(undefined, rows(["a"])), null);
});

test("what was kept reads back the same, and what is no anchor is nothing", () => {
  const kept = anchorOf(rows(["a", "b", "c"]), 50);
  assert.deepEqual(parseAnchor(JSON.parse(JSON.stringify(kept))), kept);
  assert.deepEqual(parseAnchor({ key: "a", offset: -12.4, extra: true }), { key: "a", offset: -12 }, "a whole offset, and only the two fields");
  for (const bad of [null, undefined, "a", 7, [], ["a", 1], {}, { key: "a" }, { offset: 1 }, { key: "", offset: 1 }, { key: 7, offset: 1 }, { key: "a", offset: "1" }, { key: "a", offset: Number.NaN }, { key: "a", offset: Infinity }, { key: "k".repeat(513), offset: 0 }]) {
    assert.equal(parseAnchor(bad), null, JSON.stringify(bad));
  }
});
