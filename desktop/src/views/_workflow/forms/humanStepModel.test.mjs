import test from "node:test";
import assert from "node:assert/strict";
import { pickRecommended } from "./humanStepModel.mjs";

test("one option is recommended at most: the mark moves, and pressing it again takes it off", () => {
  const options = [{ id: "a", recommended: false }, { id: "b", recommended: true }, { id: "c" }];
  assert.deepEqual(pickRecommended(options, 0).map((o) => !!o.recommended), [true, false, false]);
  assert.deepEqual(pickRecommended(options, 1).map((o) => !!o.recommended), [false, false, false], "pressed on the recommended one: none");
  assert.deepEqual(pickRecommended(options, 2).map((o) => o.recommended), [false, false, true]);
  assert.deepEqual(pickRecommended(options, 0).map((o) => o.id), ["a", "b", "c"], "ids and order kept");
  assert.notEqual(pickRecommended(options, 0), options, "a new list");
  assert.deepEqual(pickRecommended(null, 0), []);
});
