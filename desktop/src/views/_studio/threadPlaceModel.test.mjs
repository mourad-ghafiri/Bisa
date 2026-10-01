/**
 * Where a thread was being read, and how it is put back
 * (`threadPlaceModel.mjs`). Run with
 * `node --test desktop/src/views/_studio/threadPlaceModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { RESTORE_PAGES, parseThreadPlace, placeFrom, restoreStep } from "./threadPlaceModel.mjs";

const ROWS = [
  { id: "m1", top: 0, height: 100 },
  { id: "m2", top: 100, height: 40 },
  { id: "m3", top: 160, height: 200 },
];
const KEPT = { message: "m2", offset: 12 };

test("a place is the first message whose foot is below the viewport's top edge, and how far the edge is inside it", () => {
  assert.deepEqual(placeFrom(ROWS, 0), { message: "m1", offset: 0 });
  assert.deepEqual(placeFrom(ROWS, 99), { message: "m1", offset: 99 });
  assert.deepEqual(placeFrom(ROWS, 100), { message: "m2", offset: 0 }, "a row whose foot is at the edge is above it");
  assert.deepEqual(placeFrom(ROWS, 112.4), { message: "m2", offset: 12 }, "whole pixels");
});

test("an edge between two messages stands above the next one", () => {
  assert.deepEqual(placeFrom(ROWS, 150), { message: "m3", offset: -10 });
});

test("a thread with no rows, or one scrolled past its last, has no place", () => {
  assert.equal(placeFrom([], 40), null);
  assert.equal(placeFrom(ROWS, 360), null);
  assert.equal(placeFrom(null, 0), null);
});

test("a thread with nothing kept starts at its bottom", () => {
  assert.deepEqual(restoreStep({ kept: null, loading: false, ids: ["m1"], hasOlder: true, pagesLoaded: 0 }), { do: "bottom", forget: false });
  assert.deepEqual(restoreStep({ kept: undefined, loading: true, ids: [], hasOlder: false, pagesLoaded: 0 }), { do: "bottom", forget: false });
});

test("a kept message in the page is scrolled to", () => {
  assert.deepEqual(restoreStep({ kept: KEPT, loading: false, ids: ["m1", "m2", "m3"], hasOlder: true, pagesLoaded: 0 }), { do: "scroll", message: "m2", offset: 12 });
});

test("an older one is paged to, four pages at most", () => {
  assert.equal(RESTORE_PAGES, 4);
  for (let pages = 0; pages < RESTORE_PAGES; pages++) {
    assert.deepEqual(restoreStep({ kept: KEPT, loading: false, ids: ["m8", "m9"], hasOlder: true, pagesLoaded: pages }), { do: "older" }, `after ${pages} pages`);
  }
  assert.deepEqual(restoreStep({ kept: KEPT, loading: false, ids: ["m2", "m8"], hasOlder: true, pagesLoaded: 3 }), { do: "scroll", message: "m2", offset: 12 }, "a page that brings it ends the paging");
});

test("past the bound the place is forgotten", () => {
  assert.deepEqual(restoreStep({ kept: KEPT, loading: false, ids: ["m8", "m9"], hasOlder: true, pagesLoaded: RESTORE_PAGES }), { do: "bottom", forget: true });
  assert.deepEqual(restoreStep({ kept: KEPT, loading: false, ids: ["m8", "m9"], hasOlder: false, pagesLoaded: 0 }), { do: "bottom", forget: true }, "a thread read to its start that does not hold it never will");
});

test("nothing moves while the first page is out", () => {
  assert.deepEqual(restoreStep({ kept: KEPT, loading: true, ids: [], hasOlder: false, pagesLoaded: 0 }), { do: "wait" });
  assert.deepEqual(restoreStep({ kept: KEPT, loading: true, ids: ["m2"], hasOlder: false, pagesLoaded: 0 }), { do: "wait" });
});

test("what is no place reads back as nothing", () => {
  for (const raw of [null, undefined, "m2", 12, [], ["m2", 12], {}, { message: "m2" }, { offset: 12 }, { message: "", offset: 0 }, { message: 7, offset: 0 }, { message: "m2", offset: "12" }, { message: "m2", offset: Number.NaN }, { message: "m2", offset: Infinity }, { message: "m2", offset: 2_000_000 }, { message: "x".repeat(300), offset: 0 }]) {
    assert.equal(parseThreadPlace(raw), null, JSON.stringify(raw) ?? String(raw));
  }
  assert.deepEqual(parseThreadPlace({ message: "m2", offset: 12.6, more: true }), { message: "m2", offset: 13 });
  assert.deepEqual(parseThreadPlace({ message: "m3", offset: -10 }), { message: "m3", offset: -10 });
  assert.deepEqual(parseThreadPlace(JSON.parse(JSON.stringify(placeFrom(ROWS, 112)))), KEPT, "what was kept reads back as it was");
});
