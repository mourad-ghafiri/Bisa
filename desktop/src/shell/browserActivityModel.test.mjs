/**
 * Which tabs an agent is working in. Run with `node --test desktop/src/shell/browserActivityModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { NO_ACTIVITY, began, busyKeys, ended, forgotten } from "./browserActivityModel.mjs";

test("a request begins and ends on a tab; two on one tab end one at a time; a tab at zero is forgotten", () => {
  let s = began(NO_ACTIVITY, "b1");
  assert.deepEqual(busyKeys(s), ["b1"]);
  s = began(s, "b1");
  s = ended(s, "b1");
  assert.deepEqual(busyKeys(s), ["b1"], "one of two requests ended: the agent is still at work");
  s = ended(s, "b1");
  assert.deepEqual(busyKeys(s), [], "a tab at zero is not a key");
  assert.equal(ended(s, "b1"), s, "an end nobody began is nothing");
});

test("the busy tabs are listed; a closed tab is forgotten whatever was counted on it", () => {
  let s = began(began(began(NO_ACTIVITY, "b1"), "b2"), "b2");
  assert.deepEqual(busyKeys(s), ["b1", "b2"]);
  s = forgotten(s, "b2");
  assert.deepEqual(busyKeys(s), ["b1"]);
  assert.equal(forgotten(s, "b9"), s, "forgetting a stranger changes nothing");
  assert.deepEqual(NO_ACTIVITY, {}, "the state starts empty and is never mutated");
  assert.ok(Object.isFrozen(NO_ACTIVITY));
});
