/**
 * A pane's slot is published, never looked up. Run with
 * `node --test desktop/src/shell/auxSlotModel.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";

import { createSlot } from "./auxSlotModel.mjs";

test("a subscriber hears a new element, hears it go, and never hears the same element twice", () => {
  const slot = createSlot();
  const heard = [];
  slot.subscribe(() => heard.push(slot.current()));
  assert.equal(slot.current(), null);
  const first = { id: "slot-1" };
  slot.publish(first);
  slot.publish(first);
  assert.deepEqual(heard, [first], "the same element twice is not a change");
  slot.publish(null);
  assert.deepEqual(heard, [first, null], "the pane went");
  slot.publish(undefined);
  assert.deepEqual(heard, [first, null], "gone twice is once");
});

test("a pane that closed and opened again publishes a new element, and a late subscriber reads the current one", () => {
  const slot = createSlot();
  const first = { id: "slot-1" };
  const second = { id: "slot-2" };
  slot.publish(first);
  slot.publish(null);
  slot.publish(second);
  assert.equal(slot.current(), second, "the portal follows the element that is there now");
  let heard = 0;
  const off = slot.subscribe(() => {
    heard += 1;
  });
  slot.publish(null);
  off();
  slot.publish(first);
  assert.equal(heard, 1, "an unsubscribed listener hears nothing more");
});
