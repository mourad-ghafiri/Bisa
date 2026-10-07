/**
 * One counting rule for every surface that tallies sessions: the footer,
 * the node overlay, the resource overlay, the tray and the pet agree on one
 * fixture. Run with `npm test` from `desktop/`.
 */

import { strict as assert } from "node:assert";
import { test } from "node:test";

import { OFF_CHECKOUT_KINDS, busyScopes, harnessRows, isHarnessProcess, liveRows, processRows, scopeOf, sessionCounts, workingCount } from "./sessionCountsModel.mjs";
import { footerSessions } from "./footerSessionsModel.mjs";
import { attributeShares } from "./resourceModel.mjs";
import { workingCount as trayWorking } from "./trayModel.mjs";
import { claimedSessions } from "../views/_workbench/workstreamSessionsModel.mjs";

const row = (id, kind, state, over = {}) => ({ id, kind, state: { state }, harness: "claude-code", agent: "general-agent", ...over });
const FIXTURE = [
  row("w1", "worker", "running", { workstream: "ws1", goal: "g1", pid: 11 }),
  row("w2", "worker", "idle", { workstream: "ws1", goal: "g1" }),
  row("d1", "guided", "thinking", { agent: "workflow-agent", goal: "g2", pid: 12 }),
  row("a1", "ask", "thinking", { pid: 13, origin: { origin: "ask", purpose: { kind: "classifier" } } }),
  row("t1", "conversation", "thinking", { conversation: "c1", origin: { origin: "turn", scope: "c1" } }),
  row("term1", "terminal", "running", { agent: null, workstream: "ws1", pid: 14 }),
  row("term2", "terminal", "running", { agent: null, workstream: "ws1" }),
  row("gone", "worker", "done", { workstream: "ws1" }),
];
const TERMINALS = [{ key: "tab1", scope: "workstream", id: "ws1", harness: "claude-code", liveness: { status: "live" }, sessionId: "term1" }];

test("the footer lists every harness process the engine drives or a tab claims — a worker, a design wake, an ask, a claimed terminal — idle included, never a turn", () => {
  const claimed = claimedSessions(FIXTURE, TERMINALS);
  assert.deepEqual([...OFF_CHECKOUT_KINDS], ["guided", "ask"]);
  assert.equal(isHarnessProcess(FIXTURE[0], claimed), true);
  assert.equal(isHarnessProcess(FIXTURE[2], claimed), true, "a design wake is a harness process");
  assert.equal(isHarnessProcess(FIXTURE[3], claimed), true, "so is a one-shot ask");
  assert.equal(isHarnessProcess(FIXTURE[4], claimed), false, "a turn is its conversation's");
  assert.equal(isHarnessProcess(FIXTURE[5], claimed), true, "a terminal a tab claims");
  assert.equal(isHarnessProcess(FIXTURE[6], claimed), false, "a terminal no tab claims is nowhere");
  assert.deepEqual(
    harnessRows(FIXTURE, TERMINALS).map((r) => r.id),
    ["w1", "w2", "d1", "a1", "term1"],
  );
});

test("the counts agree: the footer's rows are the counts' harnesses, the node's live, the overlay's processes, the tray's working", () => {
  const working = { c1: ["general-agent"], "g:9": ["planner"] };
  const counts = sessionCounts(FIXTURE, TERMINALS, working);
  assert.equal(counts.harnesses, footerSessions(TERMINALS, FIXTURE).harnesses.length, "the footer draws the counts' harnesses");
  assert.equal(counts.live, liveRows(FIXTURE).length);
  assert.equal(counts.live, FIXTURE.length - 1, "every row but the done one");
  assert.deepEqual(processRows(FIXTURE).map((r) => r.id), ["w1", "d1", "a1", "term1"]);
  const shares = attributeShares(
    processRows(FIXTURE).map((r) => ({ root: { kind: "session", pid: r.pid }, pid: r.pid, cpu_percent: 1, mem_bytes: 1 })),
    FIXTURE,
    TERMINALS,
  );
  assert.deepEqual(shares.map((s) => s.sessionId), ["w1", "d1", "a1", "term1"], "an ask with a pid is attributed, never an anonymous ended row");
  assert.equal(counts.processes, shares.length);
  // Busy rows: w1 and the two terminals running, d1, a1 and t1 thinking = 6; the turn's scope c1 is already named by its row; g:9 is a hint only.
  assert.deepEqual([...busyScopes(FIXTURE)].sort(), ["c1", "g1", "g2"]);
  assert.equal(scopeOf(FIXTURE[4]), "c1", "a turn's scope is the bus's string");
  assert.equal(scopeOf(FIXTURE[2]), "g2", "a design wake's scope is its goal");
  assert.equal(scopeOf(FIXTURE[7]), null, "a worker in no goal names no scope");
  assert.equal(workingCount(FIXTURE, working), 6 + 1, "a thinking turn whose scope is also hinted counts once");
  assert.equal(counts.working, trayWorking(FIXTURE, working), "the tray reads the same number");
  assert.equal(workingCount(null, null), 0);
});
