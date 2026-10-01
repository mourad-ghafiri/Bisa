/**
 * Where the Inbox keeps what it remembers of one row. Run with
 * `node --test desktop/src/views/inboxPlaceModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { KeptMemory } from "../shell/keptMemoryModel.mjs";
import { rowPlace } from "./inboxPlaceModel.mjs";

test("a row's memory is kept beneath the Inbox's place, by the row's key", () => {
  assert.equal(rowPlace("/inbox", "01GOAL"), "/inbox/01GOAL");
  assert.notEqual(rowPlace("/inbox", "01GOAL"), rowPlace("/inbox", "01WORKFLOW"), "each row its own");
  assert.equal(rowPlace("/inbox", "01GOAL"), rowPlace("/inbox", "01GOAL"), "and the same one every time");
});

test("no row is the Inbox's own place", () => {
  for (const none of [null, undefined, ""]) assert.equal(rowPlace("/inbox", none), "/inbox");
});

test("a key longer than a place may be is cut, never refused", () => {
  const place = rowPlace("/inbox", "k".repeat(1000));
  assert.equal(place.length, "/inbox/".length + 256);
});

test("forgetting the Inbox's place forgets every row's with it, and no other screen's", () => {
  const stored = new Map();
  const storage = { getItem: (k) => stored.get(k) ?? null, setItem: (k, v) => void stored.set(k, v), removeItem: (k) => void stored.delete(k) };
  const memory = new KeptMemory({
    key: "test.view.state",
    version: 1,
    caps: { places: 16, valueBytes: 1024, totalBytes: 16 * 1024 },
    hands: { storage: () => storage, now: () => 0, later: () => 0, cancel: () => {} },
  });
  memory.keep(rowPlace("/inbox", "01GOAL"), "notices:opened", ["k1"]);
  memory.keep(rowPlace("/inbox", "01WORKFLOW"), "notices:all", true);
  memory.keep("/inboxes", "opened", ["k9"]);
  assert.equal(memory.forgetUnder("/inbox/"), 2);
  assert.deepEqual(memory.keptPlaces(), ["/inboxes"], "a place that only begins the same way is another screen's");
});
