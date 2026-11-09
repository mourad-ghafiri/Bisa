/**
 * One send per press, tested where the rule lives. Run with `npm test` from
 * `desktop/` (the script preloads the catalogue).
 */

import { strict as assert } from "node:assert";
import { test } from "node:test";

import { createLatch, once, uploadsKey } from "./verbLatch.mjs";

test("a second press while the node answers sends nothing", async () => {
  const latch = createLatch();
  let sent = 0;
  let answer;
  const node = () => new Promise((resolve) => {
    sent += 1;
    answer = resolve;
  });
  const first = once(latch, "stop", node);
  assert.equal(latch.holds("stop"), true);
  assert.equal(await once(latch, "stop", node), false, "held: not sent again");
  assert.equal(sent, 1);
  // Another verb is another key.
  const other = once(latch, "restart", async () => {});
  assert.equal(await other, true);
  answer();
  assert.equal(await first, true);
  assert.equal(latch.holds("stop"), false, "free once the node answered");
  assert.equal(await once(latch, "stop", async () => {}), true, "and may be sent again");
});

test("a refusal frees the key too", async () => {
  const latch = createLatch();
  await assert.rejects(once(latch, "promote", async () => {
    throw new Error("refused");
  }));
  assert.equal(latch.holds("promote"), false);
  assert.equal(latch.take("promote"), true);
});

test("the same files are the same send, whatever re-rendered in between", () => {
  const a = [{ sha256: "aa", name: "brief.pdf" }, { sha256: "bb", name: "spec.md" }];
  assert.equal(uploadsKey(a), "aa,bb");
  assert.equal(uploadsKey([...a]), uploadsKey(a));
  assert.notEqual(uploadsKey([a[0]]), uploadsKey(a));
  assert.equal(uploadsKey(null), "");
});
