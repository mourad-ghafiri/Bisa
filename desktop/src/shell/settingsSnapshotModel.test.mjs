/**
 * The settings store's rules. Run with
 * `node --test desktop/src/shell/settingsSnapshotModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { EMPTY_ENTRY, keyOf, reading, refused, settled } from "./settingsSnapshotModel.mjs";

test("a read is keyed by the workspace or the project", () => {
  assert.equal(keyOf(null), "workspace");
  assert.equal(keyOf(undefined), "workspace");
  assert.equal(keyOf(""), "workspace");
  assert.equal(keyOf("01J8PROJECT"), "01J8PROJECT");
});

test("the first read is loading, a later one refreshing over the last answer", () => {
  assert.deepEqual(reading(EMPTY_ENTRY), { ...EMPTY_ENTRY, loading: true, refreshing: false });
  const held = settled(EMPTY_ENTRY, [{ key: "a", value: 1 }], 100);
  assert.deepEqual(reading(held), { ...held, loading: false, refreshing: true });
});

test("a read that answers the same list keeps the list's identity; a differing one replaces it", () => {
  const first = settled(EMPTY_ENTRY, [{ key: "a", value: 1, origin: "default" }], 100);
  assert.equal(first.loading, false);
  assert.equal(first.at, 100);
  const same = settled(reading(first), [{ key: "a", value: 1, origin: "default" }], 200);
  assert.equal(same.data, first.data, "the same list by value keeps its identity");
  assert.equal(same.at, 200);
  assert.equal(same.refreshing, false);
  const changed = settled(reading(first), [{ key: "a", value: 2, origin: "machine" }], 300);
  assert.notEqual(changed.data, first.data);
  assert.deepEqual(changed.data, [{ key: "a", value: 2, origin: "machine" }]);
});

test("a refused read keeps the last answer and names the reason", () => {
  const held = settled(EMPTY_ENTRY, [{ key: "a", value: 1 }], 100);
  const r = refused(reading(held), new Error("the node refused"));
  assert.equal(r.data, held.data);
  assert.equal(r.error, "Error: the node refused");
  assert.equal(r.loading, false);
  assert.equal(r.refreshing, false);
  const cold = refused(reading(EMPTY_ENTRY), "offline");
  assert.equal(cold.data, null);
  assert.equal(cold.error, "offline");
});
