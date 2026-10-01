import assert from "node:assert/strict";
import { test } from "node:test";
import { BADGE_MAX, badgeText } from "./badgeModel.mjs";

test("a badge counts to its size's cap and then says *and more*", () => {
  assert.deepEqual(BADGE_MAX, { md: 99, sm: 9 });
  assert.equal(badgeText(7), "7");
  assert.equal(badgeText(99), "99");
  assert.equal(badgeText(100), "99+");
  // The mark in an icon's corner: a digit and a plus, never three characters over a 15px glyph.
  assert.equal(badgeText(9, "sm"), "9");
  assert.equal(badgeText(10, "sm"), "9+");
  assert.equal(badgeText(4213, "sm"), "9+");
  assert.ok(badgeText(4213, "sm").length <= 2);
});

test("an explicit cap wins, and nothing to count draws nothing", () => {
  assert.equal(badgeText(12, "sm", 99), "12");
  assert.equal(badgeText(1200, "md", 999), "999+");
  assert.equal(badgeText(12, "md", 0), "12", "a cap that is no cap falls back to the size's");
  for (const nothing of [0, -3, Number.NaN, Number.POSITIVE_INFINITY, undefined, null]) assert.equal(badgeText(nothing), null, String(nothing));
  assert.equal(badgeText(3.9), "3", "a count is whole");
});
