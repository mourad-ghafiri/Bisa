/**
 * An edit handed to an agent is followed to its end: the state words are the
 * session vocabulary's, a roster move settles the record only for that
 * agent's turn in that conversation, live to settled, after the ask — and the words
 * say how it ended.
 *
 * Run with `npm test` from `desktop/`.
 */

import { strict as assert } from "node:assert";
import { test } from "node:test";

import { STATES } from "../../ui/sessionState.mjs";
import { LIVE, SETTLED, recordKey, settledTone, settledWords, settles } from "./agentEditsModel.mjs";

test("live and settled together are the session vocabulary, and neither overlaps", () => {
  assert.deepEqual([...LIVE, ...SETTLED].sort(), [...STATES].sort());
  assert.ok(LIVE.every((s) => !SETTLED.includes(s)));
  assert.equal(recordKey("conversation", "c1", "www/index.html"), "conversation:c1|www/index.html");
});

const record = { scope: "conversation", id: "c1", agentId: "designer", at: 1000 };
const row = (state, over = {}) => ({ id: "s1", state: { state }, since: 1010, agent: "designer", conversation: "c1", workstream: "w1", started: 1005, ...over });

test("a move from live to settled by the asked agent in the asked conversation settles the record", () => {
  assert.ok(settles(record, { state: "running" }, row("idle")));
  assert.ok(settles(record, { state: "thinking" }, row("done")));
  assert.ok(settles(record, { state: "waiting" }, row("failed")));
  assert.ok(settles(record, { state: "starting" }, row("aborted")));
  assert.ok(settles(record, null, row("idle")), "a session first seen already at rest, newer than the ask, settles it");
});

test("anything else does not settle it", () => {
  assert.ok(!settles(record, { state: "running" }, row("thinking")), "still live");
  assert.ok(!settles(record, { state: "idle" }, row("done")), "not live before — a stranger's rest");
  assert.ok(!settles(record, { state: "running" }, row("idle", { agent: "general-agent" })), "another agent");
  assert.ok(!settles(record, { state: "running" }, row("idle", { conversation: "c2" })), "another conversation");
  assert.ok(!settles(record, { state: "running" }, row("idle", { since: 900 })), "a rest older than the ask");
  assert.ok(!settles({ ...record, scope: "workstream" }, { state: "running" }, row("idle")), "an edit is a conversation's, never a checkout's");
});

test("the words say how it ended and the tone follows", () => {
  assert.equal(settledWords("Designer", "www/index.html", "idle"), "Designer finished with index.html — keep or undo the change in the conversation, or in the file.");
  assert.equal(settledWords("Designer", "www/index.html", "done"), "Designer finished with index.html — keep or undo the change in the conversation, or in the file.");
  assert.equal(settledWords("Designer", "www/index.html", "failed"), "Designer failed while editing index.html — the Agent panel has its words.");
  assert.equal(settledWords("Designer", "index.html", "aborted"), "Designer was stopped before it finished index.html.");
  assert.equal(settledTone("idle"), "ok");
  assert.equal(settledTone("failed"), "error");
  assert.equal(settledTone("aborted"), "info");
});
