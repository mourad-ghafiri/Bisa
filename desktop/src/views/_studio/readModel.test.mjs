/**
 * When a conversation reads itself. Run with `node --test desktop/src/views/_studio/readModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { NEAR_BOTTOM_PX, READ_AFTER_MS, atBottomOf, readWanted, rowRead, shownToReader } from "./readModel.mjs";

test("a read is wanted for unread messages, or for a row unread by a gate or a notice alone — never for a read row with nothing new", () => {
  assert.ok(readWanted(2, null), "messages unread, no row known");
  assert.ok(readWanted(0, { read: false }), "a row moved by a gate or a notice, no unread message");
  assert.ok(readWanted(1, { read: true }), "the count outranks a stale row");
  assert.ok(!readWanted(0, { read: true }));
  assert.ok(!readWanted(0, null), "nothing known, nothing wanted");
  assert.ok(!readWanted(undefined, undefined));
});

test("the newest message is seen when the window is visible, the thread is at its bottom and the first page has landed", () => {
  assert.ok(shownToReader({ visible: true, atBottom: true, loaded: true }));
  assert.ok(!shownToReader({ visible: false, atBottom: true, loaded: true }), "a hidden window shows nothing");
  assert.ok(!shownToReader({ visible: true, atBottom: false, loaded: true }), "scrolled up to read back, the newest is off screen");
  assert.ok(!shownToReader({ visible: true, atBottom: true, loaded: false }), "an empty viewport during the first fetch shows nothing");
  assert.ok(!shownToReader(null));
  assert.equal(READ_AFTER_MS, 800, "the beat a passing row is spared");
});

test("a viewport is at its bottom within the tolerance", () => {
  assert.ok(atBottomOf(1000, 1000 - 400, 400), "exactly at the bottom");
  assert.ok(atBottomOf(1000, 1000 - 400 - (NEAR_BOTTOM_PX - 1), 400), "a little above still reads the newest");
  assert.ok(!atBottomOf(1000, 1000 - 400 - NEAR_BOTTOM_PX, 400), "the tolerance is exclusive");
  assert.ok(!atBottomOf(1000, 0, 400), "at the top");
  assert.ok(atBottomOf(300, 0, 400), "a page shorter than the viewport is all on screen");
});

test("marking a row read patches that row alone and leaves an unknown or read row as it was", () => {
  const rows = [
    { key: "a", read: false, unread_count: 3, unread_notices: 1 },
    { key: "b", read: false, unread_count: 1, unread_notices: 0 },
  ];
  const next = rowRead(rows, "a");
  assert.notEqual(next, rows);
  assert.deepEqual(next[0], { key: "a", read: true, unread_count: 0, unread_notices: 0 });
  assert.equal(next[1], rows[1], "the other row is the same object");
  assert.equal(rowRead(rows, "zzz"), rows, "an unknown key changes nothing");
  assert.equal(rowRead(next, "a"), next, "a read row changes nothing");
});

test("the new marker sits on the first unread message, and nowhere when nothing or everything is unread", async () => {
  const { firstUnreadId } = await import("./readModel.mjs");
  const messages = [{ id: "a" }, { id: "b" }, { id: "c" }, { id: "d" }];
  assert.equal(firstUnreadId(messages, 1), "d");
  assert.equal(firstUnreadId(messages, 3), "b");
  assert.equal(firstUnreadId(messages, 0), null);
  assert.equal(firstUnreadId(messages, 4), null, "the whole window unread: the boundary is off the top");
  assert.equal(firstUnreadId(messages, 9), null);
  assert.equal(firstUnreadId([], 1), null);
  assert.equal(firstUnreadId(messages, 2.7), "c", "a count is whole");
  assert.equal(firstUnreadId(messages, Number.NaN), null);
});
