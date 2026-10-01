import test from "node:test";
import assert from "node:assert/strict";

import { label as stateLabel } from "../../ui/sessionState.mjs";
import { WORK_KINDS, claimedSessions, isDrawn, visibleSessionRows, workstreamSessionRows } from "./workstreamSessionsModel.mjs";

const term = (key, id, harness = null, status = "live", running = null) => ({ key, scope: "workstream", id, harness, running, liveness: { status, code: 0 }, openedAt: 100, exitedAt: null });
const sess = (id, workstream, state, extra = {}) => ({ id, kind: "worker", workstream, harness: "claude-code", agent: null, state, since: 5, started: 1, work_item: null, goal: null, children: [], ...extra });

test("a session row carries the model the harness runs on, its sub-agents the same, and none until the harness says", () => {
  const rows = workstreamSessionRows(
    [sess("s1", "w1", { state: "thinking" }, { agent: "dev", model: "anthropic/claude-opus-5", children: [{ id: "c1", name: "explore", description: "", state: { state: "idle" }, since: 6 }] }), sess("s2", "w1", { state: "idle" })],
    [],
    "w1",
  );
  assert.equal(rows.find((r) => r.id === "s1").model, "anthropic/claude-opus-5", "as the harness names it");
  assert.equal(rows.find((r) => r.id === "s1/c1").model, "anthropic/claude-opus-5", "a sub-agent on its parent's model");
  assert.equal(rows.find((r) => r.id === "s2").model, null, "nothing guessed");
});

test("a session row carries the effort the session runs at, its sub-agents the same, and none when the row has none", () => {
  const rows = workstreamSessionRows(
    [sess("s1", "w1", { state: "thinking" }, { model: "claude-opus-5-5[1m]", effort: "high", children: [{ id: "c1", name: "explore", description: "", state: { state: "idle" }, since: 6 }] }), sess("s2", "w1", { state: "idle" }, { model: "claude-haiku-4-5" })],
    [],
    "w1",
  );
  assert.equal(rows.find((r) => r.id === "s1").effort, "high", "as the node says it, fitted already");
  assert.equal(rows.find((r) => r.id === "s1/c1").effort, "high", "a sub-agent at its parent's effort");
  assert.equal(rows.find((r) => r.id === "s2").effort, null, "a model that takes none runs at none");
});

test("a workstream's rows: its terminals first, then each agent with its sub-agents", () => {
  const rows = workstreamSessionRows(
    [
      sess("s1", "w1", { state: "running", tool: "Edit" }, {
        agent: "dev",
        children: [{ id: "c1", name: "explore", description: "map the crate", state: { state: "thinking" }, since: 6 }],
      }),
      sess("s9", "elsewhere", { state: "running" }),
    ],
    [term("t1", "w1", "codex"), term("t2", "elsewhere")],
    "w1",
  );
  assert.deepEqual(
    rows.map((r) => [r.kind, r.id, r.parent]),
    [
      ["terminal", "t1", null],
      ["agent", "s1", null],
      ["agent", "s1/c1", "s1"],
    ],
    "only this workstream's rows, terminals first, the sub-agent nested under its session",
  );
  const agent = rows.find((r) => r.id === "s1");
  assert.equal(agent.label, "dev", "the persona names the row");
  assert.equal(agent.activity, "running Edit");
  assert.equal(agent.harness, "claude-code");
  const sub = rows.find((r) => r.id === "s1/c1");
  assert.equal(sub.label, "explore");
  assert.equal(stateLabel({ state: "running", tool: "sub-agent", args: "explore, plan" }), "running sub-agent", "a delegating parent reads its own word");
  assert.equal(sub.activity, "map the crate", "a sub-agent's description, else its state word");
  assert.equal(sub.harness, "claude-code", "the sub-agent wears its parent's harness");
  const shell = rows.find((r) => r.kind === "terminal");
  assert.equal(shell.label, "codex");
  assert.equal(shell.liveness.status, "live");
});

test("a waiting session carries its gate id; a plain shell has no harness", () => {
  const rows = workstreamSessionRows(
    [sess("s1", "w1", { state: "waiting", on: { on: "permission", tool: "Bash", gate_id: "g1" } })],
    [term("t1", "w1", null)],
    "w1",
  );
  const agent = rows.find((r) => r.kind === "agent");
  assert.equal(agent.gateId, "g1");
  assert.equal(agent.activity, "waiting on you — permission: Bash");
  assert.equal(rows.find((r) => r.kind === "terminal").label, "shell");
});

test("an empty workstream has no rows", () => {
  assert.deepEqual(workstreamSessionRows([], [], "w1"), []);
  assert.deepEqual(workstreamSessionRows(null, null, "w1"), []);
});

test("a session row carries its sub-agent count; a childless one is zero", () => {
  const rows = workstreamSessionRows(
    [
      sess("s1", "w1", { state: "running" }, {
        children: [
          { id: "c1", name: "a", description: "", state: { state: "running" }, since: 6 },
          { id: "c2", name: "b", description: "", state: { state: "thinking" }, since: 6 },
        ],
      }),
      sess("s2", "w1", { state: "idle" }),
    ],
    [],
    "w1",
  );
  assert.equal(rows.find((r) => r.id === "s1").childCount, 2, "the count is the sub-agents under it");
  assert.equal(rows.find((r) => r.id === "s2").childCount, 0, "a childless session counts zero");
});

test("visibleSessionRows hides a folded harness's sub-agents and nothing else", () => {
  const rows = workstreamSessionRows(
    [
      sess("s1", "w1", { state: "running" }, {
        children: [{ id: "c1", name: "explore", description: "map", state: { state: "thinking" }, since: 6 }],
      }),
      sess("s2", "w1", { state: "running" }, {
        children: [{ id: "c9", name: "review", description: "read", state: { state: "running" }, since: 6 }],
      }),
    ],
    [term("t1", "w1", "codex")],
    "w1",
  );
  // Folding s1 drops only s1's sub-agent; s2's stays, and every top-level row stays.
  const collapsed = new Set(["s1"]);
  assert.deepEqual(
    visibleSessionRows(rows, collapsed).map((r) => r.id),
    ["t1", "s1", "s2", "s2/c9"],
  );
  // A predicate works the same as a Set.
  assert.deepEqual(
    visibleSessionRows(rows, (id) => id === "s2").map((r) => r.id),
    ["t1", "s1", "s1/c1", "s2"],
  );
  // Nothing folded: the full tree.
  assert.deepEqual(visibleSessionRows(rows, new Set()).map((r) => r.id), ["t1", "s1", "s1/c1", "s2", "s2/c9"]);
});

test("a harness in a terminal is drawn once — as its session's row, which remembers its tab", () => {
  const rows = workstreamSessionRows(
    [
      sess("i1", "w1", { state: "waiting", on: { on: "permission", tool: "Bash", gate_id: null } }, {
        kind: "terminal",
        children: [{ id: "a1", name: "explore", description: "find the router", state: { state: "running", tool: "Read", args: "x" }, since: 6 }],
      }),
    ],
    [{ ...term("t1", "w1", "claude-code"), sessionId: "i1" }, term("t2", "w1", null)],
    "w1",
  );
  assert.deepEqual(
    rows.map((r) => [r.kind, r.id, r.parent]),
    [
      ["terminal", "t2", null],
      ["agent", "i1", null],
      ["agent", "i1/a1", "i1"],
    ],
    "the reported tab is not a terminal row; the plain shell still is",
  );
  const row = rows.find((r) => r.id === "i1");
  assert.equal(row.terminalKey, "t1", "the row knows the tab to open");
  assert.equal(row.activity, "waiting on you — permission: Bash");
  assert.equal(row.gateId, null, "a terminal harness's permission is answered in the terminal, not through a gate");
  assert.equal(rows.find((r) => r.id === "i1/a1").terminalKey, "t1", "so does its sub-agent");
  assert.equal(rows.find((r) => r.id === "t2").terminalKey, undefined);
});

test("a tab whose session the roster does not know is a terminal row — a node that restarted forgot it", () => {
  const rows = workstreamSessionRows([], [{ ...term("t1", "w1", "claude-code"), sessionId: "gone" }], "w1");
  assert.deepEqual(rows.map((r) => [r.kind, r.id]), [["terminal", "t1"]]);
});

test("the tab is the row: a terminal session no tab claims is not drawn, and a claimed tab is never also a terminal", () => {
  const sessions = [
    sess("mine", "w1", { state: "idle" }, { kind: "terminal" }),
    sess("orphan", "w1", { state: "starting" }, { kind: "terminal" }),
    sess("worker", "w1", { state: "thinking" }),
  ];
  const tabs = [{ ...term("t1", "w1", "claude-code"), sessionId: "mine" }, term("t2", "w1", null)];
  const claimed = claimedSessions(sessions, tabs);
  assert.deepEqual([...claimed.entries()], [["mine", "t1"]]);
  assert.equal(isDrawn(sessions[0], claimed), true, "claimed: drawn through its tab");
  assert.equal(isDrawn(sessions[1], claimed), false, "unclaimed terminal: mid-open or an orphan, not standing here");
  assert.equal(isDrawn(sessions[2], claimed), true, "an engine session needs no tab");
  const rows = workstreamSessionRows(sessions, tabs, "w1");
  assert.deepEqual(
    rows.map((r) => [r.kind, r.id]),
    [
      ["terminal", "t2"],
      ["agent", "mine"],
      ["agent", "worker"],
    ],
    "one row per thing standing here: the plain shell, the claimed harness once, the worker",
  );
  assert.equal(rows.find((r) => r.id === "mine").terminalKey, "t1");
  // A tab whose session id names nothing in the roster claims nothing.
  assert.equal(claimedSessions(sessions, [{ ...term("t9", "w1", "codex"), sessionId: "nope" }]).size, 0);
});

test("a held ended row stays the tab's: a harness that exited on its own reads its end beside its tab", () => {
  const rows = workstreamSessionRows(
    [sess("mine", "w1", { state: "failed", reason: "exited with status 1" }, { kind: "terminal" })],
    [{ ...term("t1", "w1", "claude-code", "exited"), sessionId: "mine" }],
    "w1",
  );
  assert.deepEqual(rows.map((r) => [r.kind, r.id]), [["agent", "mine"]], "the exited tab is not a second row");
  assert.equal(rows[0].activity, "failed: exited with status 1");
});

test("a harness typed into a plain shell is a harness terminal row — its mark, its label, no roster row", () => {
  const rows = workstreamSessionRows([], [term("t1", "ws1", null, "live", "claude-code"), term("t2", "ws1")], "ws1");
  assert.deepEqual(rows.map((r) => [r.kind, r.label, r.harness]), [
    ["terminal", "claude-code", "claude-code"],
    ["terminal", "shell", null],
  ]);
  // The launch's own harness wins over what the table shows.
  const launched = workstreamSessionRows([], [term("t3", "ws1", "codex", "live", "claude-code")], "ws1");
  assert.equal(launched[0].harness, "codex");
});

test("a conversation's turn and a note's answer standing in the checkout are never its rows — a worker and a claimed terminal are", () => {
  const sessions = [
    sess("turn", "w1", { state: "running", tool: "Edit" }, { kind: "conversation", agent: "developer", conversation: "c1" }),
    sess("answer", "w1", { state: "thinking" }, { kind: "note", agent: "scribe" }),
    sess("design", "w1", { state: "thinking" }, { kind: "guided", agent: "workflow-agent" }),
    sess("worker", "w1", { state: "thinking" }),
    sess("mine", "w1", { state: "idle" }, { kind: "terminal" }),
  ];
  const tabs = [{ ...term("t1", "w1", "claude-code"), sessionId: "mine" }];
  const claimed = claimedSessions(sessions, tabs);
  assert.deepEqual([...WORK_KINDS], ["worker", "terminal"], "the work kinds, and no other");
  assert.equal(isDrawn(sessions[0], claimed), false, "a conversation's turn is the conversation's, reached through the Agent panel");
  assert.equal(isDrawn(sessions[1], claimed), false, "a note's answer is the note's");
  assert.equal(isDrawn(sessions[2], claimed), false, "a design wake stands in no checkout");
  assert.equal(isDrawn(sessions[3], claimed), true);
  assert.equal(isDrawn(sessions[4], claimed), true);
  assert.deepEqual(
    workstreamSessionRows(sessions, tabs, "w1").map((r) => [r.kind, r.id]),
    [
      ["agent", "worker"],
      ["agent", "mine"],
    ],
    "harnesses and terminals only: never a conversation",
  );
  assert.ok(Object.isFrozen(WORK_KINDS));
});

test("a sub-agent is a row while it is live or failed, carries its own start, and is gone once finished", () => {
  const rows = workstreamSessionRows(
    [
      sess("s1", "w1", { state: "running", tool: "sub-agent", args: "explore" }, {
        children: [
          { id: "c1", name: "explore", description: "map the crate", state: { state: "thinking" }, since: 6, started: 6 },
          { id: "c2", name: "plan", description: "", state: { state: "done" }, since: 7, started: 3 },
          { id: "c3", name: "review", description: "", state: { state: "failed", reason: "r" }, since: 8, started: 4 },
        ],
      }),
    ],
    [],
    "w1",
  );
  assert.deepEqual(
    rows.filter((r) => r.parent).map((r) => r.id),
    ["s1/c1", "s1/c3"],
    "the done one has left; the failed one stays red until the engine's turn boundary",
  );
  assert.equal(rows.find((r) => r.id === "s1").childCount, 2, "the count is the rows shown");
  assert.equal(rows.find((r) => r.id === "s1/c1").started, 6, "a sub-agent counts from its spawn instant");
  assert.equal(rows.find((r) => r.id === "s1/c3").started, 4);
});

test("a session whose claiming tab exited reads as the tab says: ended as of the exit, no sub-agents, no live clock", () => {
  const exitedTab = { ...term("t1", "w1", "claude-code", "exited"), sessionId: "s1", exitedAt: 900, liveness: { status: "exited", code: 1 } };
  const rows = workstreamSessionRows(
    [sess("s1", "w1", { state: "running", tool: "Edit" }, { kind: "terminal", since: 500, children: [{ id: "c1", name: "explore", description: "", state: { state: "thinking" }, since: 600, started: 600 }] })],
    [exitedTab],
    "w1",
  );
  const agent = rows.find((r) => r.id === "s1");
  assert.deepEqual(agent.state, { state: "failed", reason: "exited with status 1" });
  assert.equal(agent.since, 900);
  assert.equal(agent.activity, stateLabel({ state: "failed", reason: "exited with status 1" }));
  assert.equal(agent.childCount, 0);
  assert.equal(agent.terminalKey, "t1", "the row still opens its tab");
  assert.ok(!rows.some((r) => r.parent === "s1"), "its sub-agents went with it");
  const liveTab = { ...term("t1", "w1", "claude-code"), sessionId: "s1" };
  const liveRows = workstreamSessionRows([sess("s1", "w1", { state: "running", tool: "Edit" }, { kind: "terminal" })], [liveTab], "w1");
  assert.deepEqual(liveRows.find((r) => r.id === "s1").state, { state: "running", tool: "Edit" }, "a live tab leaves the roster's word");
});
