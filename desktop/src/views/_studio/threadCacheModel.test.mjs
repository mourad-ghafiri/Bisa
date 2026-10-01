/**
 * The messages a thread had, and how the newest page joins them
 * (`threadCacheModel.mjs`). Run with
 * `node --test desktop/src/views/_studio/threadCacheModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { MAX_MESSAGES, MAX_THREADS, joinAround, joinNewest, joinReactions, keptThread, threadKey, trimmed, nudgeRead } from "./threadCacheModel.mjs";

const m = (n, more = {}) => ({ id: `m${n}`, created_at: n, content: `said ${n}`, retracted: false, ...more });
const run = (from, to) => Array.from({ length: to - from + 1 }, (_, i) => m(from + i));
const ids = (messages) => messages.map((x) => x.id);

test("a thread is kept under the chat's own key, and a hosted one under its host as well", () => {
  assert.equal(threadKey("channel", "c1"), "channel:c1");
  assert.equal(threadKey("channel", "c1", null), "channel:c1");
  assert.equal(threadKey("channel", "c1", "host1"), "host1/channel:c1");
});

test("the caps are twenty-four threads and three hundred messages each", () => {
  assert.equal(MAX_THREADS, 24);
  assert.equal(MAX_MESSAGES, 300);
});

test("nothing shown yet takes the page as the thread", () => {
  const first = joinNewest({ shown: [], page: run(1, 3), pageSize: 3 });
  assert.deepEqual(ids(first.messages), ["m1", "m2", "m3"]);
  assert.equal(first.hasOlder, true, "a full page may have more above it");
  assert.equal(joinNewest({ shown: [], page: run(1, 2), pageSize: 3 }).hasOlder, false, "a short page is the whole thread");
});

test("the newest page replaces where it overlaps and keeps what is older", () => {
  const joined = joinNewest({ shown: run(1, 6), page: run(5, 8), pageSize: 4, hasOlder: true });
  assert.deepEqual(ids(joined.messages), ["m1", "m2", "m3", "m4", "m5", "m6", "m7", "m8"]);
  assert.equal(joined.restarted, false);
  assert.equal(joined.hasOlder, true, "what the thread knew of its start stays");
  assert.equal(joinNewest({ shown: run(1, 6), page: run(5, 8), pageSize: 4, hasOlder: false }).hasOlder, false, "a thread read to its start is not read again from there");
});

test("a row the page no longer holds inside its span is gone, and one that landed after the page was read is kept", () => {
  const page = [m(5), m(7), m(8), m(9)];
  const joined = joinNewest({ shown: [...run(1, 9), m(10)], page, pageSize: 4, hasOlder: true });
  assert.deepEqual(ids(joined.messages), ["m1", "m2", "m3", "m4", "m5", "m7", "m8", "m9", "m10"]);
});

test("a short page is the whole thread", () => {
  const joined = joinNewest({ shown: run(1, 6), page: run(4, 6), pageSize: 4, hasOlder: true });
  assert.deepEqual(ids(joined.messages), ["m4", "m5", "m6"]);
  assert.equal(joined.hasOlder, false);
});

test("more landed than a page holds starts over", () => {
  const joined = joinNewest({ shown: run(1, 6), page: run(20, 23), pageSize: 4, hasOlder: false });
  assert.deepEqual(ids(joined.messages), ["m20", "m21", "m22", "m23"]);
  assert.equal(joined.restarted, true);
  assert.equal(joined.hasOlder, true, "older rows are paged in again");
});

test("a retracted message replaces its row", () => {
  const page = [m(5), m(6, { retracted: true, content: "" }), m(7), m(8)];
  const joined = joinNewest({ shown: run(1, 6), page, pageSize: 4, hasOlder: true });
  assert.deepEqual(ids(joined.messages), ["m1", "m2", "m3", "m4", "m5", "m6", "m7", "m8"]);
  assert.equal(joined.messages.find((x) => x.id === "m6").retracted, true);
  assert.equal(joined.messages.filter((x) => x.id === "m6").length, 1, "drawn once");
});

test("a page read again above the newest replaces its span and keeps both sides", () => {
  const page = [m(3), m(4, { retracted: true }), m(5)];
  const joined = joinAround({ shown: run(1, 9), page });
  assert.deepEqual(ids(joined), ["m1", "m2", "m3", "m4", "m5", "m6", "m7", "m8", "m9"]);
  assert.equal(joined.find((x) => x.id === "m4").retracted, true);
  const shown = run(1, 3);
  assert.deepEqual(joinAround({ shown, page: [] }), shown, "a page with nothing in it changes nothing");
});

test("the reactions of the page replace, and those on the messages kept from before stay", () => {
  const r = (id, target) => ({ id, target_id: target, emoji: "+", author: "a", retracted: false });
  const joined = joinNewest({ shown: run(1, 6), page: run(5, 8), pageSize: 4, hasOlder: true });
  const reactions = joinReactions({ shown: [r("r1", "m2"), r("r2", "m5"), r("r3", "m6")], page: [r("r3", "m6"), r("r4", "m8")], messages: joined.messages, landed: run(5, 8) });
  assert.deepEqual(reactions.map((x) => x.id), ["r1", "r3", "r4"], "a mark the page no longer has on its own message is gone");
  const restarted = joinNewest({ shown: run(1, 6), page: run(20, 23), pageSize: 4 });
  assert.deepEqual(joinReactions({ shown: [r("r1", "m2")], page: [], messages: restarted.messages, landed: run(20, 23) }), [], "a mark on a message no longer held goes with it");
});

test("the newest are kept past the cap", () => {
  const long = run(1, 10);
  assert.deepEqual(ids(trimmed(long, 4)), ["m7", "m8", "m9", "m10"]);
  assert.equal(trimmed(long, 10), long, "a list within the cap is the same list");
  assert.equal(trimmed(run(1, MAX_MESSAGES + 5)).length, MAX_MESSAGES);
});

test("a thread cut to its cap has more above, and keeps only the reactions on what it holds", () => {
  const thread = { messages: run(1, 6), reactions: [{ id: "r1", target_id: "m1" }, { id: "r2", target_id: "m6" }], hasOlder: false };
  const kept = keptThread(thread, 3);
  assert.deepEqual(ids(kept.messages), ["m4", "m5", "m6"]);
  assert.deepEqual(kept.reactions.map((x) => x.id), ["r2"]);
  assert.equal(kept.hasOlder, true);
  assert.equal(keptThread(thread, 6), thread, "a thread within the cap is kept as it stands");
});

test("a frame that names a message reads that one row; a snapshot, a reaction, a retraction, a hosted scope or no frame reads the newest page", async () => {
  const { readFileSync } = await import("node:fs");
  const kinds = readFileSync(new URL("../../../../crates/bisa-core/src/kind.rs", import.meta.url), "utf8");
  assert.ok(kinds.includes("pub const KIND_MESSAGE: u16 = 3407;"), "the message kind is the core's");
  assert.deepEqual(nudgeRead({ scope: "c1", kind: 3407, event_id: "ab12" }, false), { read: "one", id: "ab12" });
  assert.deepEqual(nudgeRead({ scope: "c1", kind: 3407, event_id: "ab12" }, true), { read: "page" }, "a hosted scope is read over the guest wire, a page at a time");
  assert.deepEqual(nudgeRead({ scope: "c1", kind: 33405, snapshot: true }, false), { read: "page" });
  assert.deepEqual(nudgeRead({ scope: "c1", kind: 7, event_id: "r1" }, false), { read: "page" }, "a reaction is no message: asking for it as one would be a 404 nobody hears");
  assert.deepEqual(nudgeRead({ scope: "c1", kind: 3408, event_id: "x1" }, false), { read: "page" }, "a retraction: the page says what it withdrew");
  assert.deepEqual(nudgeRead({ scope: "c1", kind: 3407 }, false), { read: "page" }, "a frame that names no event");
  assert.deepEqual(nudgeRead(undefined, false), { read: "page" }, "the bus came back");
  // The hook: a one-message read is superseded by nothing — a reply in three messages is three reads, in frame order — and one that fails reads the page.
  const hook = readFileSync(new URL("./useScopeMessages.ts", import.meta.url), "utf8");
  const nudge = hook.slice(hook.indexOf("const nudge = useCallback("), hook.indexOf("useConversationEvents(nudge"));
  assert.ok(nudge.includes("const plan = nudgeRead(frame, host != null);"));
  assert.ok(!nudge.includes("live.current?.abort()"), "a row's read aborts no read before it");
  assert.ok(nudge.includes("queue.current = queue.current") && nudge.includes(".then(async () => {"), "the reads stand in one line: rows of one second join in the order they were posted");
  assert.ok(nudge.includes("if (!ctrl.signal.aborted) readNewest();"), "a row that could not be read is caught up by the page, never left out in silence");
  const page = hook.slice(hook.indexOf("const readNewest = useCallback("), hook.indexOf("const nudge = useCallback("));
  assert.ok(page.includes("live.current?.abort();") && page.includes("log.debug("), "a later page read supersedes an earlier one, and a page that did not come is said in the log");
});

