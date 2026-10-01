/**
 * `sameJsonList` as facts. Run with `node --test desktop/src/shell/snapshotEqual.test.mjs`.
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { sameJsonList } from "./snapshotEqual.mjs";

test("the same reference is equal without walking it", () => {
  const a = [{ id: "s1" }];
  assert.equal(sameJsonList(a, a), true);
});

test("empty lists are equal", () => {
  assert.equal(sameJsonList([], []), true);
});

test("different lengths are never equal", () => {
  assert.equal(sameJsonList([{ id: "a" }], [{ id: "a" }, { id: "b" }]), false);
});

test("element-for-element equal by value, not reference", () => {
  const a = [{ id: "s1", state: { state: "running" } }];
  const b = [{ id: "s1", state: { state: "running" } }];
  assert.equal(a[0] === b[0], false, "distinct objects");
  assert.equal(sameJsonList(a, b), true, "but equal by JSON");
});

test("a changed field is not equal", () => {
  const a = [{ id: "s1", state: { state: "running" } }];
  const b = [{ id: "s1", state: { state: "waiting" } }];
  assert.equal(sameJsonList(a, b), false);
});

test("order matters — a reordered roster is a change", () => {
  const a = [{ id: "a" }, { id: "b" }];
  const b = [{ id: "b" }, { id: "a" }];
  assert.equal(sameJsonList(a, b), false);
});
