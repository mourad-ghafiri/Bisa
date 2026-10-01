/**
 * The drop target under a pointer is the smallest one containing it. Run
 * with `node --test desktop/src/ui/dnd/collisionModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { smallestFirst } from "./collisionModel.mjs";

test("the smallest box wins, a box unknown sorts last, equals keep their order, and the input is not touched", () => {
  const rects = new Map([
    ["column", { width: 300, height: 900 }],
    ["card", { width: 280, height: 80 }],
    ["row", { width: 280, height: 24 }],
    ["twin", { width: 24, height: 280 }],
  ]);
  const within = ["column", "card", "row", "ghost", "twin"];
  assert.deepEqual(smallestFirst(within, (id) => rects.get(id)), ["row", "twin", "card", "column", "ghost"]);
  assert.deepEqual(within, ["column", "card", "row", "ghost", "twin"], "the input keeps its order");
  assert.deepEqual(smallestFirst([], () => null), []);
  assert.deepEqual(smallestFirst(["a", "b"], () => null), ["a", "b"], "no boxes at all: the order given");
});
