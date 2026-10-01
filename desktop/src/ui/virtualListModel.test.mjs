/**
 * A virtual list's window is arithmetic over a measured viewport. Run with
 * `node --test desktop/src/ui/virtualListModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { clampScroll, indexAt, localFrame, spacerHeight, windowAt, windowOf } from "./virtualListModel.mjs";

const ROW = 28;
const OVERSCAN = 8;
const win = (scrollTop, count, viewHeight = 500) => windowOf({ scrollTop, viewHeight, rowHeight: ROW, count, overscan: OVERSCAN });
/** The row under the viewport's bottom edge. */
const edgeRow = (scrollTop, viewHeight = 500) => Math.floor((scrollTop + viewHeight) / ROW);

test("an unmeasured viewport draws no window; a measured one always covers its bottom edge", () => {
  assert.deepEqual(win(0, 5000, 0), { first: 0, last: 0 }, "no height: nothing, never twenty guessed rows");
  assert.deepEqual(win(0, 0), { first: 0, last: 0 }, "no rows: nothing");
  const top = win(0, 5000);
  assert.equal(top.first, 0);
  assert.ok(top.last >= Math.ceil(500 / ROW) + OVERSCAN && top.last <= 5000, `the first screen and the overscan: ${top.last}`);
  const deep = win(2000, 5000);
  assert.ok(deep.first <= Math.floor(2000 / ROW) && deep.first >= Math.floor(2000 / ROW) - OVERSCAN);
  assert.ok(edgeRow(2000) < deep.last, "the row under the bottom edge is inside");
  const bottom = win(5000 * ROW - 500, 5000);
  assert.equal(bottom.last, 5000, "at the very bottom the last row is the last row");
  assert.ok(bottom.first < 5000);
});

test("a count that grows or shrinks under a fixed scrollTop still covers the edge, and the scroll is clamped", () => {
  const few = win(1000, 200);
  const many = win(1000, 5000);
  assert.ok(edgeRow(1000) < few.last && few.last <= 200, "two hundred rows: the edge is inside");
  assert.ok(edgeRow(1000) < many.last, "five thousand rows, the same scroll: still inside");
  assert.equal(clampScroll(9000, 200 * ROW, 500), 200 * ROW - 500, "a scroll past a shrunken total is pulled back to the last screen");
  assert.equal(clampScroll(10, 5600, 500), 10, "a scroll inside the total stays");
  assert.equal(clampScroll(5, 100, 500), 0, "a total shorter than the viewport scrolls nowhere");
  assert.equal(clampScroll(-3, 5600, 500), 0);
});

test("measured offsets window by bisection", () => {
  const offsets = [0, 28, 84, 112, 200, 260];
  assert.equal(indexAt(offsets, 0), 0);
  assert.equal(indexAt(offsets, 27), 0);
  assert.equal(indexAt(offsets, 28), 1, "an exact end is the next row");
  assert.equal(indexAt(offsets, 199), 3);
  assert.equal(indexAt(offsets, 999), 4, "past the end is the last row");
  assert.equal(indexAt(offsets, -5), 0, "before the start is the first");
  const w = windowAt({ offsets, scrollTop: 30, viewHeight: 100, overscan: 0 });
  assert.equal(w.first, 1);
  assert.ok(w.last > indexAt(offsets, 130), "the row under the bottom edge is inside");
  assert.deepEqual(windowAt({ offsets, scrollTop: 0, viewHeight: 0, overscan: 0 }), { first: 0, last: 0 }, "unmeasured: nothing");
  assert.deepEqual(windowAt({ offsets: [0], scrollTop: 0, viewHeight: 100, overscan: 2 }), { first: 0, last: 0 }, "no rows: nothing");
});

test("an unbounded list windows against the panel that scrolls it", () => {
  assert.deepEqual(localFrame({ scrollTop: 300, viewHeight: 600, offsetTop: 250, total: 2000 }), { scrollTop: 50, viewHeight: 600 });
  assert.equal(localFrame({ scrollTop: 0, viewHeight: 600, offsetTop: 250, total: 2000 }).scrollTop, 0, "the panel above the list: the list's top");
  assert.equal(localFrame({ scrollTop: 9000, viewHeight: 600, offsetTop: 250, total: 2000 }).scrollTop, 2000, "the panel past the list: the list's end");
  assert.equal(localFrame({ scrollTop: 100, viewHeight: -1, offsetTop: 0, total: 50 }).viewHeight, 0);
});

test("the spacer is every row's height", () => {
  assert.equal(spacerHeight(0, ROW), 0);
  assert.equal(spacerHeight(3, ROW), 84);
  assert.equal(spacerHeight(-1, ROW), 0);
});
