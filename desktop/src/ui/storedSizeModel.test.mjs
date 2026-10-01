/**
 * A stored pane size is read within the pane's own bounds. Run with
 * `node --test desktop/src/ui/storedSizeModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { clampSize, clampStored } from "./storedSizeModel.mjs";

const bounds = { initial: 280, min: 220, max: 420 };

test("a value inside the bounds is kept, one outside is clamped", () => {
  assert.equal(clampStored("300", bounds), 300);
  assert.equal(clampStored("100", bounds), 220);
  assert.equal(clampStored("9999", bounds), 420);
});

test("nothing, garbage, zero and a negative are the default", () => {
  assert.equal(clampStored(null, bounds), 280);
  assert.equal(clampStored(undefined, bounds), 280);
  assert.equal(clampStored("", bounds), 280);
  assert.equal(clampStored("wide", bounds), 280);
  assert.equal(clampStored("0", bounds), 280);
  assert.equal(clampStored("-40", bounds), 280);
  assert.equal(clampStored("Infinity", bounds), 280);
});

test("without bounds any positive number is kept", () => {
  assert.equal(clampStored("5000", { initial: 380 }), 5000);
});

test("a size already read is held within bounds, and no bound means no hold", () => {
  assert.equal(clampSize(300, { min: 220, max: 420 }), 300);
  assert.equal(clampSize(100, { min: 220, max: 420 }), 220);
  assert.equal(clampSize(9999, { min: 220, max: 420 }), 420);
  assert.equal(clampSize(9999, { min: 220 }), 9999);
  assert.equal(clampSize(-5, {}), 0);
});
