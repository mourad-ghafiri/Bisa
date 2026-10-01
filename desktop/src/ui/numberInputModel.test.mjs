/**
 * The number field's one rule, tested where it lives.
 *
 * Run with `npm test` from `desktop/`.
 */

import { strict as assert } from "node:assert";
import { test } from "node:test";

import { parseBounded } from "./numberInputModel.mjs";

test("a number is held to its bounds and truncated", () => {
  assert.equal(parseBounded("12", { min: 0, max: 255 }), 12);
  assert.equal(parseBounded(" 7 ", { min: 1 }), 7);
  assert.equal(parseBounded("300", { min: 0, max: 255 }), 255);
  assert.equal(parseBounded("-4", { min: 0 }), 0);
  assert.equal(parseBounded("2.9", { min: 0 }), 2, "truncated, not rounded");
});

test("nothing, or not a number, is the fallback", () => {
  assert.equal(parseBounded("", { fallback: 3 }), 3);
  assert.equal(parseBounded("abc", { fallback: 1 }), 1);
  assert.equal(parseBounded(null, { fallback: 5 }), 5);
  assert.equal(parseBounded(undefined), 0);
  assert.equal(parseBounded("Infinity", { fallback: 9 }), 9);
});
