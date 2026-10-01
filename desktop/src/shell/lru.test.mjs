/**
 * The LRU map as facts. Run with `node --test desktop/src/shell/lru.test.mjs`.
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { Lru } from "./lru.mjs";

test("a non-positive capacity is refused", () => {
  assert.throws(() => new Lru(0));
  assert.throws(() => new Lru(-1));
  assert.throws(() => new Lru(1.5));
});

test("get returns what was set, and undefined for a miss", () => {
  const lru = new Lru(3);
  lru.set("a", 1);
  assert.equal(lru.get("a"), 1);
  assert.equal(lru.get("b"), undefined);
});

test("the oldest key is evicted past the cap", () => {
  const lru = new Lru(2);
  lru.set("a", 1);
  lru.set("b", 2);
  lru.set("c", 3); // evicts "a", the oldest
  assert.equal(lru.has("a"), false);
  assert.deepEqual(lru.keys(), ["b", "c"]);
  assert.equal(lru.size, 2);
});

test("get marks a key most-recently-used, sparing it the next eviction", () => {
  const lru = new Lru(2);
  lru.set("a", 1);
  lru.set("b", 2);
  lru.get("a"); // "a" is now newest; "b" is oldest
  lru.set("c", 3); // evicts "b"
  assert.equal(lru.has("b"), false);
  assert.equal(lru.has("a"), true);
  assert.deepEqual(lru.keys(), ["a", "c"]);
});

test("has does not change recency", () => {
  const lru = new Lru(2);
  lru.set("a", 1);
  lru.set("b", 2);
  lru.has("a"); // must NOT rescue "a"
  lru.set("c", 3); // still evicts "a", the true oldest
  assert.equal(lru.has("a"), false);
});

test("re-setting an existing key refreshes it and replaces the value", () => {
  const lru = new Lru(2);
  lru.set("a", 1);
  lru.set("b", 2);
  lru.set("a", 9); // "a" newest again, value replaced
  lru.set("c", 3); // evicts "b"
  assert.equal(lru.get("a"), 9);
  assert.equal(lru.has("b"), false);
});

test("setCapacity shrinks the cap and evicts the oldest at once", () => {
  const lru = new Lru(4);
  lru.set("a", 1);
  lru.set("b", 2);
  lru.set("c", 3);
  lru.setCapacity(1); // drops the two oldest, keeps the newest
  assert.deepEqual(lru.keys(), ["c"]);
  assert.equal(lru.size, 1);
  // A non-positive capacity is ignored.
  lru.setCapacity(0);
  assert.equal(lru.capacity, 1);
});

test("delete removes a key and reports whether it was present", () => {
  const lru = new Lru(2);
  lru.set("a", 1);
  assert.equal(lru.delete("a"), true);
  assert.equal(lru.delete("a"), false);
  assert.equal(lru.size, 0);
});
