import test from "node:test";
import assert from "node:assert/strict";
import { roundRect, sameRect, slotStyle } from "./centerSlotModel.mjs";

test("equal rects are the same rect, so an observer tick with the same numbers is silent", () => {
  const a = { left: 1, top: 2, width: 3, height: 4 };
  assert.ok(sameRect(a, { ...a }));
  assert.ok(!sameRect(a, { ...a, width: 5 }));
  assert.ok(sameRect(null, null));
  assert.ok(!sameRect(a, null));
  assert.deepEqual(roundRect({ left: 1.4, top: 2.6, width: 3.5, height: 4.49 }), { left: 1, top: 3, width: 4, height: 4 });
  assert.equal(roundRect(null), null);
});

test("a hidden layer keeps a real box and hides with visibility, never with a zero size", () => {
  const rect = { left: 10, top: 20, width: 300, height: 200 };
  const shown = slotStyle(rect, true);
  assert.equal(shown.position, "fixed");
  assert.equal(shown.visibility, "visible");
  assert.equal(shown.width, 300);
  const hidden = slotStyle(rect, false);
  assert.equal(hidden.visibility, "hidden");
  assert.equal(hidden.pointerEvents, "none");
  assert.equal(hidden.width, 300, "the last box stays laid out");
  // Before any rect arrives there is still a non-zero box to measure in.
  const none = slotStyle(null, true);
  assert.equal(none.visibility, "hidden");
  assert.ok(none.width > 0 && none.height > 0);
  assert.equal(slotStyle({ left: 0, top: 0, width: 0, height: 0 }, true).visibility, "hidden");
});
