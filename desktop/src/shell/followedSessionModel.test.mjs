import { strict as assert } from "node:assert";
import { test } from "node:test";
import { chosenSession, fallbackSession, followWords, followedSession, terminalSessionOf } from "./followedSessionModel.mjs";

function row(id, state, over = {}) {
  return { id, kind: "worker", state: { state }, since: 100, harness: "claude-code", agent: "general-agent", workstream: "ws1", started: 100, cost: {}, children: [], last_activity: 100, ...over };
}

test("a chosen session is followed while it is still here", () => {
  const rows = [row("a", "idle"), row("b", "running", { since: 200 })];
  assert.equal(chosenSession("a", rows, "ws1")?.id, "a");
  assert.equal(followedSession("a", rows, "ws1")?.id, "a", "chosen beats the loudest");
});

test("a choice that left the roster, or moved to another workstream, falls back", () => {
  const rows = [row("b", "running"), row("c", "thinking", { workstream: "ws2" })];
  assert.equal(chosenSession("gone", rows, "ws1"), null);
  assert.equal(chosenSession("c", rows, "ws1"), null, "in another workstream");
  assert.equal(followedSession("gone", rows, "ws1")?.id, "b");
  assert.equal(followedSession("c", rows, "ws1")?.id, "b");
});

test("the fallback is the loudest live session, then the newest ended one", () => {
  const rows = [row("idle", "idle", { since: 300 }), row("wait", "waiting", { since: 100 }), row("run", "running", { since: 200 })];
  assert.equal(fallbackSession(rows, "ws1")?.id, "wait", "attention first");
  assert.equal(fallbackSession([row("r1", "running", { started: 100 }), row("r2", "running", { started: 200 })], "ws1")?.id, "r2", "then the one that started last");
  assert.equal(fallbackSession([row("r1", "running", { since: 100 }), row("r2", "running", { since: 900 })], "ws1")?.id, "r1", "a tool call — a newer `since` — never swaps the subject; equal starts break by id");
  assert.equal(fallbackSession([row("old", "done", { since: 100 }), row("new", "failed", { since: 200 })], "ws1")?.id, "new", "the newest ended when nothing is live");
  assert.equal(fallbackSession([row("p", "parked")], "ws1"), null, "a parked session is nobody to follow");
  assert.equal(fallbackSession([row("x", "running", { workstream: "ws2" })], "ws1"), null);
});

test("off a workstream, or in an empty one, nothing is followed", () => {
  assert.equal(followedSession("a", [row("a", "running")], null), null);
  assert.equal(followedSession(null, [], "ws1"), null);
});

test("the centre's terminal tab names its harness session", () => {
  const terminals = [{ key: "t1", sessionId: "s1" }, { key: "t2", sessionId: null }];
  assert.equal(terminalSessionOf(terminals, "terminal:t1"), "s1");
  assert.equal(terminalSessionOf(terminals, "terminal:t2"), null, "a plain shell");
  assert.equal(terminalSessionOf(terminals, "terminal:nope"), null);
  assert.equal(terminalSessionOf(terminals, "src/main.rs"), null, "a file is not a tab");
  assert.equal(terminalSessionOf(terminals, null), null);
});

test("the words name the agent and the harness, or the harness alone", () => {
  assert.equal(followWords({ agent: "general-agent", harness: "claude-code" }), "general-agent on claude-code");
  assert.equal(followWords({ agent: null, harness: "codex" }), "codex");
  assert.equal(followWords({ agent: "codex", harness: "codex" }), "codex", "a harness standing for itself is not said twice");
});
