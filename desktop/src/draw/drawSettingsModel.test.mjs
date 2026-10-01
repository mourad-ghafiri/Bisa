import { strict as assert } from "node:assert";
import { test } from "node:test";
import { DEFAULT_SNAPSHOT_WIDTH, MAX_SNAPSHOT_WIDTH, MIN_SNAPSHOT_WIDTH, POLICIES, policyWords, snapshotWidth } from "./drawSettingsModel.mjs";

test("the snapshot width is the setting within bounds, and the default for anything that is not a number", () => {
  assert.equal(snapshotWidth(undefined), DEFAULT_SNAPSHOT_WIDTH);
  assert.equal(snapshotWidth(""), DEFAULT_SNAPSHOT_WIDTH);
  assert.equal(snapshotWidth(true), DEFAULT_SNAPSHOT_WIDTH);
  assert.equal(snapshotWidth("abc"), DEFAULT_SNAPSHOT_WIDTH);
  assert.equal(snapshotWidth(10), MIN_SNAPSHOT_WIDTH);
  assert.equal(snapshotWidth(99999), MAX_SNAPSHOT_WIDTH);
  assert.equal(snapshotWidth("1920.4"), 1920);
});

test("the three policies each have a sentence", () => {
  assert.deepEqual([...POLICIES], ["everyone", "assigned", "nobody"]);
  const words = POLICIES.map(policyWords);
  assert.equal(new Set(words).size, 3);
  assert.ok(words.every((w) => w.length > 10));
});
