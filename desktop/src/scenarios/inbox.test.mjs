/**
 * The Inbox as a person lives in it, stepped through the models the screen
 * and the sidebar read — the list arrives, frames patch it in place, a frame
 * that says more than a row can show asks for the list, reading a row moves
 * the badge, a filter narrows without losing the selection, a harness's wait
 * comes and goes. No DOM.
 *
 * Run with `node --test desktop/src/scenarios/inbox.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { readFileSync } from "node:fs";

import { FIRST_CONN, connAfter } from "../busModel.mjs";
import { reloadOnReconnect } from "../shell/workspaceLoadModel.mjs";
import { applyDelta, counts, doorLabel, doorOf, emptyWords, filterOf, groupRows, isHandled, isNewestRead, markBatches, needsReload, nextSelection, rowState, sourceIdOf, summarize, unreadKeys, visibleRows, wantsList, withReadMark } from "../views/_studio/inboxModel.mjs";
import { inboxBadge } from "../shell/sidebarModel.mjs";
import { trayReport } from "../shell/trayModel.mjs";
import { badgeText } from "../ui/badgeModel.mjs";

/** The source the node gives a row of each kind — a conversation's is its origin's, `messages` here for one about nothing in particular. */
const SOURCE = { goal: "goals", channel: "messages", dm: "messages", conversation: "messages", workstream: "projects", project: "projects", workflow: "workflows", people: "people", session: "projects" };
const row = (over = {}) => ({ key: "K", kind: "goal", source: SOURCE[over.kind ?? "goal"], title: "t", latest_at: 100, unread_count: 0, read: true, handled: false, mentioned: false, needs_action: [], notices: [], unread_notices: 0, ...over });
const ask = { gate_id: "G1", gate_kind: "escalation", question: "Which database?", subject: "step:R/ask", opened_at: 90 };
const frame = (over = {}) => ({ key: "K", kind: "goal", unread_count: 0, needs_action_count: 0, latest_at: 100, read: true, handled: false, ...over });

/** What the screen does with one bus frame: patch in place, and say whether the list is wanted. */
function hear(rows, busFrame) {
  if (busFrame.stream === "inbox") return { rows: applyDelta(rows, busFrame.payload), reload: needsReload(rows, busFrame.payload) };
  return { rows, reload: wantsList(busFrame) };
}

test("a morning in the Inbox: what is owed first, read as you go, the badge following, the selection staying put", () => {
  // The list arrives: one goal asking, one channel unread, one goal long read.
  let rows = [row({ key: "asking", needs_action: [ask], read: false, unread_count: 2 }), row({ key: "lounge", kind: "channel", read: false, unread_count: 5 }), row({ key: "done", read: true })];
  assert.deepEqual(
    groupRows(visibleRows(rows, "all", "any")).map((g) => [g.id, g.rows.map((r) => r.key)]),
    [
      ["waiting", ["asking"]],
      ["new", ["lounge"]],
      ["kept", ["done"]],
    ],
  );
  let badge = inboxBadge(rows);
  // Two counts, never one sum: the owed one in the accent, the unread one beside it, neutral.
  assert.deepEqual([badge.needs, badge.unread, badgeText(badge.needs, "sm")], [1, 1, "1"]);

  // The person opens the first row: selected, and it stays selected under every filter that still holds it.
  let selected = nextSelection(visibleRows(rows, "all", "any"), rows, null);
  assert.equal(selected, "asking");
  selected = nextSelection(visibleRows(rows, "needs_you", "any"), rows, selected);
  assert.equal(selected, "asking");
  selected = nextSelection(visibleRows(rows, "unread", "messages"), rows, selected);
  assert.equal(selected, "asking", "a filter that hides the open row does not close it");

  // The channel is read elsewhere: its frame patches the row where it sits — no list, no new order under the reader.
  let heard = hear(rows, { stream: "inbox", payload: frame({ key: "lounge", kind: "channel", read: true, unread_count: 0 }) });
  assert.equal(heard.reload, false);
  assert.equal(heard.rows[1].key, "lounge");
  assert.equal(rowState(heard.rows[1]), "read");
  assert.strictEqual(heard.rows[0], rows[0], "the rows a frame does not name are the same objects");
  rows = heard.rows;
  badge = inboxBadge(rows);
  assert.deepEqual([badge.needs, badge.unread, badge.title], [1, 0, "1 needs you"]);

  // The question is answered: the count clears the ask, the row is handled, the badge is gone.
  heard = hear(rows, { stream: "inbox", payload: frame({ key: "asking", needs_action_count: 0, handled: true, read: true }) });
  rows = heard.rows;
  assert.equal(heard.reload, false);
  assert.equal(rowState(rows[0]), "read", "nothing is owed any more");
  assert.ok(isHandled(rows[0]), "and the row says it was decided — beside its state, not as one");
  assert.deepEqual([inboxBadge(rows).needs, inboxBadge(rows).unread], [0, 0]);
  assert.equal(badgeText(inboxBadge(rows).needs), null, "nothing owed draws nothing");
  assert.deepEqual(unreadKeys(visibleRows(rows, "all", "any")), []);
});

test("a frame that says more than the row can show asks for the list; one that says less never invents", () => {
  const rows = [row({ key: "asking", needs_action: [ask] })];
  // A second question on the same goal: the frame counts two, the row holds one.
  assert.equal(hear(rows, { stream: "inbox", payload: frame({ key: "asking", needs_action_count: 2 }) }).reload, true);
  // A notice the row does not hold.
  assert.equal(hear(rows, { stream: "inbox", payload: frame({ key: "asking", needs_action_count: 1, notice_count: 1, unread_notices: 1 }) }).reload, true);
  // A row nobody holds: a new goal, a new conversation.
  assert.equal(hear(rows, { stream: "inbox", payload: frame({ key: "new-goal" }) }).reload, true);
  // Patched meanwhile, the question the row holds keeps its words.
  const patched = hear(rows, { stream: "inbox", payload: frame({ key: "asking", needs_action_count: 2 }) }).rows;
  assert.equal(patched[0].needs_action.length, 1);
  assert.equal(patched[0].needs_action[0].question, "Which database?");
});

test("a harness waiting in its terminal is a row while it waits and gone when it does not", () => {
  let rows = [row({ key: "done" })];
  const waiting = { stream: "inbox", payload: frame({ key: "S1", kind: "session", needs_action_count: 1, waiting: true }) };
  assert.equal(hear(rows, waiting).reload, true, "a wait nobody holds is a row to fetch");
  rows = [...rows, row({ key: "S1", kind: "session", waiting: { session: "S1", harness: "claude-code", words: "approval" } })];
  assert.equal(inboxBadge(rows).needs, 1);
  const over = hear(rows, { stream: "inbox", payload: frame({ key: "S1", kind: "session", waiting: false }) });
  assert.deepEqual(over.rows.map((r) => r.key), ["done"], "the wait ended: the row left, without a reload");
  assert.equal(over.reload, false);
  // A wait that ended before its row was ever shown moves nothing.
  assert.equal(hear([row({ key: "done" })], { stream: "inbox", payload: frame({ key: "S2", kind: "session", waiting: false }) }).reload, false);
});

test("the rest of the bus: a run that ended wants the list, an agent's tokens do not, a frame nobody can read is nothing", () => {
  const rows = [row()];
  assert.equal(hear(rows, { stream: "engine", payload: { payload: { type: "run_finished" } } }).reload, true);
  assert.equal(hear(rows, { stream: "engine", payload: { payload: { type: "invite_changed" } } }).reload, true, "somebody asked to join");
  assert.equal(hear(rows, { stream: "engine", payload: { payload: { type: "agent_streamed" } } }).reload, false);
  assert.equal(hear(rows, { stream: "conversation", payload: { scope: "K", snippet: "hello" } }).reload, false);
  assert.equal(hear(rows, { stream: "conversation", payload: { scope: "K", snapshot: true } }).reload, true);
  assert.equal(hear(rows, { stream: "engine", payload: null }).reload, false);
  assert.strictEqual(hear(rows, { stream: "engine", payload: null }).rows, rows);
});

test("a full Inbox marked read: a few at a time, shown before the node says so, and the list read whatever landed", () => {
  let rows = Array.from({ length: 19 }, (_, i) => row({ key: `c${i}`, kind: "channel", read: false, unread_count: 1, latest_at: 100 - i }));
  rows.push(row({ key: "asking", needs_action: [ask], read: false }));
  const visible = visibleRows(rows, filterOf("unread"), sourceIdOf(undefined));
  const keys = unreadKeys(visible);
  assert.equal(keys.length, 20);
  const batches = markBatches(keys);
  assert.ok(batches.every((b) => b.length <= 8) && batches.flat().length === 20, "never twenty requests at once");
  // Each mark is drawn as it is made: read clears what was new and never what is owed.
  for (const key of batches.flat()) rows = withReadMark(rows, key, true);
  assert.equal(counts(rows, "all").buckets.unread, 0);
  assert.equal(counts(rows, "all").buckets.needs_you, 1, "reading a question does not answer it");
  assert.deepEqual([inboxBadge(rows).needs, inboxBadge(rows).unread], [1, 0], "what is owed stays; nothing unread is left");
  assert.deepEqual([summarize(rows.at(-1)).text, summarize(rows.at(-1)).tone], ["escalation: step ask", "accent"], "the ask is still the row's line");
  // Nothing new is left under Unread, and the screen says so.
  assert.deepEqual(visibleRows(rows, "unread", "any"), []);
  assert.equal(emptyWords("unread", "any").title, "Nothing new");
});

test("two reads of the list are out at once: the newer one's answer is the list, whichever lands last", () => {
  // The screen numbers its reads; a frame asks for one, then *Mark all read* for another.
  let newest = 0;
  const ask1 = ++newest;
  const ask2 = ++newest;
  let rows = [row({ key: "a", read: false })];
  const land = (asked, answer) => {
    if (isNewestRead(asked, newest)) rows = answer;
  };
  land(ask2, [row({ key: "a", read: true })]);
  land(ask1, [row({ key: "a", read: false })]);
  assert.equal(rows[0].read, true, "the older answer put nothing back");
});

test("the node restarts while the Inbox is open: what it forgot and withdrew is read when it is back, and the badge and the icon follow", () => {
  let conn = FIRST_CONN;
  const watchers = new Set();
  const happen = (event) => {
    const next = connAfter(conn, event);
    if (next === conn) return;
    conn = next;
    for (const w of watchers) w(conn);
  };
  const watch = (cb) => {
    watchers.add(cb);
    cb(conn);
    return () => watchers.delete(cb);
  };
  // A publish gate is open and a harness waits in its terminal.
  let rows = [row({ key: "G", needs_action: [{ ...ask, gate_kind: "publish", subject: "push fix/total" }], read: false }), row({ key: "S1", kind: "session", waiting: { session: "S1", harness: "claude-code", words: "permission: Bash", workstream: "W1" } })];
  assert.equal(trayReport({ conn: "open", everOpen: true, inbox: rows }).needs, 2);
  let reads = 0;
  // The node at boot withdrew the question nobody can answer and ended the session: no frame said so.
  const afterRestart = [row({ key: "G", read: false, notices: [], needs_action: [] })];
  reloadOnReconnect(watch, () => {
    reads++;
    rows = afterRestart;
  });
  happen("attempt");
  happen("opened");
  happen("ended");
  assert.equal(trayReport({ conn, everOpen: true, inbox: rows }).state, "trouble");
  assert.equal(trayReport({ conn, everOpen: true, inbox: rows }).needs, 2, "what was owed is still said while the node is away");
  happen("attempt");
  happen("opened");
  assert.equal(reads, 1);
  assert.deepEqual(rows.map((r) => r.key), ["G"], "the terminal's wait went with its session");
  assert.equal(inboxBadge(rows).needs, 0);
  assert.equal(trayReport({ conn, everOpen: true, inbox: rows }).state, "quiet");
  const screen = readFileSync(new URL("../views/Inbox.tsx", import.meta.url), "utf8");
  assert.ok(screen.includes("useReloadOnReconnect(() => void load())"), "the screen's own list reads again too");
});

test("every row opens somewhere, in words that say where", () => {
  const doors = [
    [row({ kind: "goal" }), "Open goal", "goal"],
    [row({ kind: "workflow" }), "Open workflow", "workflow"],
    [row({ kind: "workflow", needs_action: [{ ...ask, home: { home: "run", run: "R9" } }] }), "Open workflow", "run"],
    [row({ kind: "project" }), "Open in the IDE", "workbench"],
    [row({ kind: "people" }), "Open People", "settings"],
    [row({ kind: "conversation" }), "Open the conversation", "conversation"],
  ];
  for (const [r, words, route] of doors) {
    assert.equal(doorLabel(r.kind), words);
    assert.equal(doorOf(r).route.name, route);
  }
});
