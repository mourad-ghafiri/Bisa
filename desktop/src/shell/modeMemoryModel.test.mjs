/**
 * A mode remembered per key, the rules two screens share (ide/09 §Agent
 * Mode; the Workflow screen's own switch). Run with `npm test` from `desktop/`.
 */

import { strict as assert } from "node:assert";
import { test } from "node:test";

import { DEFAULT_CAP, modeFor, modeIn, nextMode, parseRememberedModes, rememberMode } from "./modeMemoryModel.mjs";

const OFFERED = Object.freeze(["one", "two", "three"]);

test("a value is one of the modes offered, else the fallback; a setting falls the same way", () => {
  assert.equal(modeIn("two", OFFERED, "one"), "two");
  assert.equal(modeIn("nope", OFFERED, "one"), "one");
  assert.equal(modeIn(undefined, OFFERED, "one"), "one");
  assert.equal(modeFor("three", "two", OFFERED, "one"), "three", "remembered wins");
  assert.equal(modeFor("gone", "two", OFFERED, "one"), "two", "a remembered word the vocabulary lost falls to the setting");
  assert.equal(modeFor(null, "odd", OFFERED, "one"), "one", "a setting the vocabulary lacks falls to the fallback");
  assert.equal(modeFor(null, "two", ["one", "two"], "one"), "two", "over a narrower offer");
});

test("a cycle goes round the offer and starts again", () => {
  assert.equal(nextMode("one", OFFERED, "one"), "two");
  assert.equal(nextMode("three", OFFERED, "one"), "one");
  assert.equal(nextMode("nope", OFFERED, "one"), "two", "an unknown mode is the fallback, so the next is the second");
  assert.equal(nextMode("two", ["one", "two"], "one"), "one");
});

test("memory is per key, newest last, capped, unchanged when the same, and read back only when real", () => {
  let m = rememberMode({}, "workflow:a", "two");
  assert.deepEqual(m, { "workflow:a": "two" });
  assert.equal(rememberMode(m, "workflow:a", "two"), m, "unchanged is the same object");
  m = rememberMode(m, "goal:b", "three", 2);
  m = rememberMode(m, "goal:c", "one", 2);
  assert.deepEqual(Object.keys(m), ["goal:b", "goal:c"], "the oldest is forgotten past the cap");
  assert.equal(DEFAULT_CAP, 32);
  assert.deepEqual(
    parseRememberedModes({ "workflow:a": "two", "goal:b": "nope", "goal:c": "three", nope: "one", 3: "one" }, OFFERED),
    { "workflow:a": "two", "goal:c": "three" },
  );
  assert.deepEqual(parseRememberedModes(["one"], OFFERED), {});
  assert.deepEqual(parseRememberedModes(null, OFFERED), {});
});
