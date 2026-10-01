/**
 * The IDE's Agent pane over conversations: which sessions are a
 * conversation's turns and what the bar says of them, where a hand-off goes
 * (the one guess the desktop makes), the memory of which roots show the list.
 * The surface itself — the pick, the views, the words — is
 * `_studio/conversationSurfaceModel.test.mjs`'s.
 * Run with `node --test desktop/src/views/_workbench/conversationPaneModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";

import {
  LIST_VIEW,
  MAX_REMEMBERED,
  VIEW_KEY,
  currentConversation,
  handOffTarget,
  handOffWords,
  rememberedStillHere,
  parseRemembered,
  remember,
  sessionsOf,
  turnSummary,
} from "./conversationPaneModel.mjs";

const conv = (id, origin, extra = {}) => ({ id, origin, archived: false, created_at: 1, last_message_at: null, agents: [], ...extra });
const S = (id, extra = {}) => ({
  id,
  kind: "conversation",
  state: { state: "idle" },
  since: 1,
  harness: "claude-code",
  agent: "developer",
  work_item: null,
  workstream: "W1",
  project: "P1",
  goal: null,
  conversation: "c1",
  cost: { input_tokens: 0, output_tokens: 0, usd_cents: 0 },
  children: [],
  last_activity: 1,
  ...extra,
});

test("the memory keeps one choice per root, forgets on null, and drops the oldest past the cap", () => {
  assert.deepEqual(parseRemembered(null), {});
  assert.deepEqual(parseRemembered([1]), {});
  assert.deepEqual(parseRemembered({ a: "c1", b: 2, c: "" }), { a: "c1" }, "only non-empty strings are choices");
  let m = remember({}, "workstream:W1", "c1");
  assert.deepEqual(m, { "workstream:W1": "c1" });
  m = remember(m, "workstream:W1", "c2");
  assert.deepEqual(m, { "workstream:W1": "c2" }, "a new choice replaces the old");
  m = remember(m, "workstream:W1", null);
  assert.deepEqual(m, {});
  for (let i = 0; i < MAX_REMEMBERED + 3; i += 1) m = remember(m, `root-${i}`, `c-${i}`);
  assert.equal(Object.keys(m).length, MAX_REMEMBERED);
  assert.equal(m["root-0"], undefined, "the oldest went");
  assert.equal(m[`root-${MAX_REMEMBERED + 2}`], `c-${MAX_REMEMBERED + 2}`);
});

test("a checkout is on the remembered conversation while it is live in the project, else the newest whose turns run here, else the newest of the project, else none", () => {
  const rows = [
    conv("old", { kind: "workstream", id: "W1", project: "P1" }, { project: "P1", created_at: 1 }),
    conv("new", { kind: "workstream", id: "W1", project: "P1" }, { project: "P1", created_at: 2 }),
    conv("proj", { kind: "project", id: "P1" }, { project: "P1", created_at: 3, last_message_at: 3 }),
    conv("gone", { kind: "workstream", id: "W1", project: "P1" }, { project: "P1", created_at: 9, archived: true }),
    conv("sibling", { kind: "workstream", id: "W2", project: "P1" }, { project: "P1", created_at: 10 }),
    conv("elsewhere", { kind: "workstream", id: "W9", project: "P2" }, { project: "P2", created_at: 11 }),
  ];
  assert.equal(currentConversation(rows, "W1", "P1", "old").id, "old", "the choice holds");
  assert.equal(currentConversation(rows, "W1", "P1", null).id, "proj", "the newest whose turns run here — not the sibling's, though it is newer");
  assert.equal(currentConversation(rows, "W1", "P1", "gone").id, "proj", "an archived choice is no choice");
  assert.equal(currentConversation(rows, "W1", "P1", "sibling").id, "sibling", "a sibling checkout's conversation is the project's: picked, it holds");
  assert.equal(currentConversation(rows, "W1", "P1", "elsewhere").id, "proj", "another project's is never this checkout's");
  const onlySibling = [conv("sibling", { kind: "workstream", id: "W2", project: "P1" }, { project: "P1", created_at: 10 })];
  assert.equal(currentConversation(onlySibling, "W1", "P1", null).id, "sibling", "with nothing that runs here, the newest of the project");
  assert.equal(currentConversation([], "W1", "P1", "old"), null);
});

test("a conversation's turns are the roster rows that name it, sorted by attention, with their sub-agents; nothing else is", () => {
  const sessions = [
    S("idle"),
    S("waiting", { state: { state: "waiting", on: { on: "permission", tool: "Bash", gate_id: "g1" } }, children: [{ id: "a", name: "explore", description: "", state: { state: "running" }, since: 2 }] }),
    S("elsewhere", { conversation: "c2" }),
    S("worker", { kind: "worker", conversation: null, agent: null }),
    S("terminal", { kind: "terminal", conversation: null, agent: null }),
  ];
  const rows = sessionsOf(sessions, "c1");
  assert.deepEqual(rows.map((r) => r.id), ["waiting", "idle"]);
  assert.equal(rows[0].gateId, "g1");
  assert.equal(rows[0].abortable, true);
  assert.equal(rows[1].abortable, false, "idle between turns offers no Stop");
  assert.equal(rows[0].children.length, 1);
  assert.equal(rows[0].label, "developer");
  assert.equal(rows[0].terminalKey, null, "a turn is never a terminal tab");
  assert.deepEqual(sessionsOf(sessions, null), []);
  assert.deepEqual(sessionsOf(sessions, "nope"), []);
  assert.deepEqual(sessionsOf(null, "c1"), []);
});

test("the bar's summary counts the turns, names the loudest live one, and offers the one Stop ends", () => {
  const none = turnSummary([]);
  assert.equal(none.words, "");
  assert.equal(none.activity, null);
  assert.equal(none.stoppable, null);
  const rows = sessionsOf([S("a", { state: { state: "running", tool: "Bash", args: "cargo test" } }), S("b")], "c1");
  const summary = turnSummary(rows);
  assert.equal(summary.working, 1);
  assert.equal(summary.words, "1 working");
  assert.equal(summary.activity, "developer — running Bash · cargo test");
  assert.equal(summary.stoppable.id, "a");
  assert.equal(turnSummary(sessionsOf([S("b")], "c1")).words, "1 idle");
});

test("the list view is remembered per root under its own key, with the one word the memory keeps", () => {
  assert.equal(VIEW_KEY, "bisa.ide.agents.view");
  const m = remember({}, "workstream:W1", LIST_VIEW);
  assert.deepEqual(parseRemembered(JSON.parse(JSON.stringify(m))), { "workstream:W1": "list" }, "the view survives a reload");
  assert.deepEqual(remember(m, "workstream:W1", null), {}, "picking a row leaves the list");
});

test("a hand-off goes to the current conversation when its turns run here, else starts one about the checkout — never to a sibling's, never to a session", () => {
  assert.deepEqual(handOffTarget({ id: "c1", origin: { kind: "workstream", id: "W1", project: "P1" } }, "W1", "P1"), { conversation: "c1" });
  assert.deepEqual(handOffTarget({ id: "c2", origin: { kind: "project", id: "P1" } }, "W1", "P1"), { conversation: "c2" }, "the project's runs in the primary, which stands for the project");
  assert.deepEqual(
    handOffTarget({ id: "c3", origin: { kind: "workstream", id: "W2", project: "P1" } }, "W1", "P1"),
    { start: { kind: "workstream", id: "W1", project: "P1" } },
    "a sibling checkout's conversation would run the hand-off there: a new one about this checkout instead",
  );
  assert.deepEqual(handOffTarget(null, "W1", "P1"), { start: { kind: "workstream", id: "W1", project: "P1" } });
  assert.equal(handOffWords("Reviewer", false), "Sent to Reviewer.");
  assert.equal(handOffWords("Reviewer", true), "Sent to Reviewer in a new conversation.");
});

test("a remembered conversation is still the checkout's while it is live and stands in this project", () => {
  const ws = { origin: { kind: "workstream", id: "w1", project: "p1" }, project: "p1", archived: false };
  assert.equal(rememberedStillHere(ws, "p1"), true);
  assert.equal(rememberedStillHere({ origin: { kind: "project", id: "p1" }, project: "p1", archived: false }, "p1"), true, "the project's counts");
  assert.equal(rememberedStillHere({ origin: { kind: "workstream", id: "w2", project: "p1" }, project: "p1", archived: false }, "p1"), true, "a sibling checkout's is the project's: a choice holds");
  assert.equal(rememberedStillHere({ ...ws, archived: 1700000000 }, "p1"), false, "archived: choose again");
  assert.equal(rememberedStillHere({ origin: { kind: "goal", id: "g1" }, archived: false }, "p1"), false, "a goal thread stands in no project");
  assert.equal(rememberedStillHere({ origin: { kind: "project", id: "p2" }, project: "p2", archived: false }, "p1"), false, "another project's does not");
  assert.equal(rememberedStillHere(null, "p1"), false);
  assert.equal(rememberedStillHere({}, "p1"), false, "no project, no place");
});

test("a hand-off whose remembered conversation cannot be read starts a new one only when the node said it is gone — never for a node that did not answer", async () => {
  const { readFileSync } = await import("node:fs");
  const store = readFileSync(new URL("./conversationsStore.ts", import.meta.url), "utf8");
  const ensure = store.slice(store.indexOf("export async function ensureConversation"), store.indexOf("if (!current) {"));
  assert.ok(ensure.includes("if (!(e instanceof ApiError && e.status === 404)) throw e;"), "a network failure once read as *gone* and left two conversations about one checkout");
  assert.ok(!/catch \{/.test(ensure), "nothing is swallowed whole");
});
