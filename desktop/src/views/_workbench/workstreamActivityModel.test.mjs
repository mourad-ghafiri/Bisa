import test from "node:test";
import assert from "node:assert/strict";
import { fold, projectActivity, terminalState, workstreamActivity } from "./workstreamActivityModel.mjs";

const S = (id, workstream, state, extra = {}) => ({
  id, kind: "worker", state, since: 1, harness: "claude-code", work_item: null, workstream, project: "P", goal: null,
  cost: { input_tokens: 0, output_tokens: 0, usd_cents: 0 }, children: [], last_activity: 10, ...extra,
});
const T = (id, liveness, harness = "claude-code", sessionId = null) => ({
  key: `t${id}`, scope: "workstream", id, harness, label: null, resume: false, generation: 0, liveness, restoring: false, sessionId,
});
const waiting = { state: "waiting", on: { on: "gate", gate: "approval", gate_id: "g" } };

test("a shell says failed when it died badly and nothing otherwise — never running from its bytes", () => {
  assert.equal(terminalState(T("W", { status: "exited", code: 137 })), "failed");
  assert.equal(terminalState(T("W", { status: "live" })), null, "alive at its prompt, or mid-turn: the shell cannot tell, so it says nothing");
  assert.equal(terminalState(T("W", { status: "live" }, null)), null);
  assert.equal(terminalState(T("W", { status: "exited", code: 0 })), null);
  assert.equal(terminalState(T("W", { status: "unverifiable", reason: "x" })), null);
});

test("a harness opened in a terminal is a roster session: its reported state is the workstream's", () => {
  // The interactive session arrives in the roster like any other, with the
  // workstream it was opened in; the tab behind it adds no word of its own.
  const interactive = S("i1", "W", { state: "running", tool: "Bash", args: "cargo test" }, { kind: "terminal" });
  const a = workstreamActivity([interactive], [T("W", { status: "live" }, "claude-code", "i1")], "W");
  assert.equal(a.state, "running");
  assert.deepEqual(a.counts, { needsYou: 0, working: 1, done: 0, live: 1, agents: 1 });
  const idle = workstreamActivity([S("i1", "W", { state: "idle" }, { kind: "terminal" })], [T("W", { status: "live" }, "claude-code", "i1")], "W");
  assert.equal(idle.state, "idle", "a harness at its prompt reads idle because it said so");
  assert.equal(idle.counts.working, 0);
});

test("a claimed tab speaks through its row only, and an unclaimed interactive row not at all", () => {
  // The harness in the tab exited badly on its own: the roster row says so;
  // the tab's own exit code is not a second failure.
  const failed = S("i1", "W", { state: "failed", reason: "exited with status 1" }, { kind: "terminal" });
  const a = workstreamActivity([failed], [T("W", { status: "exited", code: 1 }, "claude-code", "i1")], "W");
  assert.equal(a.state, "failed");
  assert.equal(a.counts.needsYou, 1, "one failure, not two");
  // An interactive row no tab claims is mid-open or an orphan: not here.
  const orphan = S("i2", "W", { state: "starting" }, { kind: "terminal" });
  const b = workstreamActivity([orphan], [], "W");
  assert.equal(b.state, "idle");
  assert.equal(b.counts.agents, 0);
});

test("a tab that exited settles its row for the dot too: the mark agrees with the pill and the footer", () => {
  // The roster still says *running* — the node's frame is on its way — but
  // the tab is gone: done on a clean exit, failed otherwise, as `settledByTab` says.
  const running = S("i1", "W", { state: "running", tool: "Bash", args: "", tier: "exec" }, { kind: "terminal", children: [{ id: "c1", name: "explore", description: "", state: { state: "thinking" }, since: 1, started: 1 }] });
  const clean = workstreamActivity([running], [T("W", { status: "exited", code: 0 }, "claude-code", "i1")], "W");
  assert.equal(clean.state, "done");
  assert.deepEqual(clean.counts, { needsYou: 0, working: 0, done: 1, live: 0, agents: 1 }, "no sub-agent survives the process it ran in");
  const bad = workstreamActivity([running], [T("W", { status: "exited", code: 1 }, "claude-code", "i1")], "W");
  assert.equal(bad.state, "failed");
  assert.equal(bad.counts.needsYou, 1, "one failure: the settled row's, not the shell's too");
});

test("a workstream folds to its loudest state and counts what stands in it", () => {
  const sessions = [S("a", "W", waiting), S("b", "W", { state: "running", tool: "Edit", args: "" }), S("c", "elsewhere", { state: "failed", reason: "x" })];
  const a = workstreamActivity(sessions, [T("W", { status: "live" })], "W");
  assert.equal(a.state, "waiting");
  assert.deepEqual(a.counts, { needsYou: 1, working: 1, done: 0, live: 1, agents: 2 });
  const quiet = workstreamActivity([], [T("W", { status: "live" })], "W");
  assert.equal(quiet.state, "idle", "a live shell alone is idle: nothing reported");
  assert.equal(quiet.counts.agents, 0);
  assert.equal(quiet.counts.live, 1);
  const sub = workstreamActivity([S("d", "W", { state: "idle" }, { children: [{ id: "t", name: "x", description: "", state: { state: "failed", reason: "r" }, since: 1 }] })], [], "W");
  assert.equal(sub.state, "failed", "a failed sub-agent is the session's failure");
});

test("a dead shell outranks a working agent, and a project rolls its workstreams up", () => {
  const w1 = workstreamActivity([S("a", "W1", { state: "thinking" })], [T("W1", { status: "exited", code: 1 })], "W1");
  assert.equal(w1.state, "failed");
  assert.equal(w1.counts.needsYou, 1, "the bad exit wants a person");
  const w2 = workstreamActivity([S("b", "W2", { state: "running", tool: "Read", args: "" })], [], "W2");
  assert.equal(w2.state, "running");
  const p = projectActivity([w2, w1]);
  assert.equal(p.state, "failed");
  assert.equal(p.counts.needsYou, 1);
  assert.equal(p.counts.working, 2);
  assert.equal(fold([]), "idle");
  assert.equal(fold(["idle", "done", "running"]), "running", "a loader beside a tick is the loader");
  assert.equal(fold(["idle", "done"]), "done", "the tick comes when the work is over");
});

test("a sub-agent that finished never makes its workstream — or its project — done", () => {
  const parent = S("p", "W", { state: "running", tool: "sub-agent", args: "explore" }, { children: [{ id: "c", name: "explore", description: "", state: { state: "done" }, since: 1 }] });
  const a = workstreamActivity([parent], [], "W");
  assert.equal(a.state, "running");
  assert.deepEqual(a.counts, { needsYou: 0, working: 1, done: 0, live: 0, agents: 1 }, "a child is not a session of the workstream");
  assert.equal(projectActivity([a]).state, "running");
  const thinker = S("q", "W2", { state: "thinking" }, { children: [{ id: "c", name: "plan", description: "", state: { state: "done" }, since: 1 }] });
  assert.equal(workstreamActivity([thinker], [], "W2").state, "thinking");
});
