/**
 * Where a dragged item lands. Run with `node --test desktop/src/ui/dnd/sortModel.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";

import { cycle, hoverIndex, moveIndex, sortableDrop, sortableId } from "./sortModel.mjs";

test("a sortable drop takes the place of the item it lands on", () => {
  const ids = ["a", "b", "c", "d"];
  assert.deepEqual(sortableDrop(ids, "a", "c"), { from: 0, to: 2 });
  assert.deepEqual(sortableDrop(ids, "d", "a"), { from: 3, to: 0 });
  assert.equal(sortableDrop(ids, "b", "b"), null, "dropped on itself: nothing moves");
  assert.equal(sortableDrop(ids, "zz", "a"), null, "an item not in the list is another list's");
  assert.equal(sortableDrop(ids, "a", "zz"), null);
});

test("moveIndex reorders in place and returns the same list when nothing moves", () => {
  const list = ["a", "b", "c", "d"];
  assert.deepEqual(moveIndex(list, 0, 2), ["b", "c", "a", "d"]);
  assert.deepEqual(moveIndex(list, 3, 0), ["d", "a", "b", "c"]);
  assert.equal(moveIndex(list, 1, 1), list, "same place: same array");
  assert.equal(moveIndex(list, 9, 0), list, "an index that names nothing moves nothing");
  assert.deepEqual(moveIndex(list, 0, 99), ["b", "c", "d", "a"], "past the end is the end");
});

test("cycle wraps at both ends and starts at the first tab when nothing is active", () => {
  const ids = ["x", "y", "z"];
  assert.equal(cycle(ids, "x", 1), "y");
  assert.equal(cycle(ids, "z", 1), "x");
  assert.equal(cycle(ids, "x", -1), "z");
  assert.equal(cycle(ids, null, 1), "x");
  assert.equal(cycle(ids, "gone", -1), "x", "an active id no longer in the strip starts over");
  assert.equal(cycle([], null, 1), null);
});

test("an item's id is its list's unless it belongs to a family, whose id is the same in every list", () => {
  assert.equal(sortableId(null, "l1", "a"), "l1/a");
  assert.equal(sortableId(undefined, "l1", "a"), "l1/a");
  assert.equal(sortableId("board", "l1", "a"), "board/a");
  assert.equal(sortableId("board", "l2", "a"), "board/a", "the same card, another column, one id");
});

test("a foreign item hovering a list takes the slot of the item it is over, else the end", () => {
  const ids = ["a", "b", "c"];
  assert.equal(hoverIndex(ids, "b", "x"), 1);
  assert.equal(hoverIndex(ids, "a", "x"), 0);
  assert.equal(hoverIndex(ids, null, "x"), 3, "the well is the end");
  assert.equal(hoverIndex(ids, "zz", "x"), 3, "an item the list does not know is the end");
  assert.equal(hoverIndex(ids, "b", "b"), 1, "over itself, where it already is");
  assert.equal(hoverIndex([], null, "x"), 0, "an empty list has one slot");
});
