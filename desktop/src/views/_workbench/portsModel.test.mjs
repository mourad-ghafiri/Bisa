/**
 * Ports tied to workstreams, as facts. Run with
 * `node --test desktop/src/views/_workbench/portsModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";

import { attributePorts, globalPorts, groupPorts, portTitle, portUrl, portsOf, portsSummary, stopPlacedPrompt, stopPrompt, portsKeptWords } from "./portsModel.mjs";

const term = (key, id, terminalId, harness = null) => ({ key, scope: "workstream", id, harness, terminalId, label: null, resume: false, generation: 0, liveness: { status: "live" }, restoring: false });
const sess = (id, workstream, pid, harness = "claude-code") => ({ id, workstream, pid, harness, state: { state: "running" } });
const tp = (port, pid, root, process = "node") => ({ port, pid, process, root });

test("a terminal-rooted port lands on the shell's workstream", () => {
  const got = attributePorts([tp(5173, 300, { kind: "terminal", terminal_id: "term-1" }, "vite")], [term("t1", "w1", "term-1")], []);
  assert.equal(got.length, 1);
  assert.deepEqual(got[0], { port: 5173, pid: 300, process: "vite", workstream: "w1", via: { kind: "shell", key: "t1", harness: null } });
});

test("a pid-rooted port lands on the session's workstream, carrying the harness", () => {
  const got = attributePorts([tp(3000, 410, { kind: "pid", pid: 400 })], [], [sess("s1", "w2", 400, "codex")]);
  assert.deepEqual(got[0].via, { kind: "harness", session: "s1", harness: "codex" });
  assert.equal(got[0].workstream, "w2");
});

test("a port whose root the desktop no longer holds is dropped", () => {
  assert.deepEqual(attributePorts([tp(5173, 300, { kind: "terminal", terminal_id: "gone" })], [term("t1", "w1", "term-1")], []), []);
  assert.deepEqual(attributePorts([tp(3000, 410, { kind: "pid", pid: 999 })], [], [sess("s1", "w2", 400)]), []);
});

test("a terminal without a resolved pty id (never went live) roots nothing", () => {
  assert.deepEqual(attributePorts([tp(5173, 300, { kind: "terminal", terminal_id: "term-1" })], [term("t1", "w1", null)], []), []);
});

test("portsOf is one workstream's, lowest first, deduped by pid+port", () => {
  const attributed = [
    { port: 8080, pid: 2, process: "node", workstream: "w1", via: { kind: "shell", key: "t1", harness: null } },
    { port: 5173, pid: 1, process: "vite", workstream: "w1", via: { kind: "shell", key: "t1", harness: null } },
    { port: 5173, pid: 1, process: "vite", workstream: "w1", via: { kind: "harness", session: "s1", harness: "x" } },
    { port: 9000, pid: 3, process: "api", workstream: "w2", via: { kind: "shell", key: "t2", harness: null } },
  ];
  assert.deepEqual(
    portsOf(attributed, "w1").map((p) => p.port),
    [5173, 8080],
  );
});

test("the tooltip names the port, the process, its pid and who opened it", () => {
  const shell = { port: 5173, pid: 42, process: "vite", workstream: "w1", via: { kind: "shell", key: "t1", harness: null } };
  const harness = { port: 3000, pid: 7, process: "node", workstream: "w1", via: { kind: "harness", session: "s1", harness: "claude-code" } };
  assert.equal(portTitle(shell), ":5173 · vite (pid 42) · shell");
  assert.equal(portTitle(harness, { "claude-code": "Claude Code" }), ":3000 · node (pid 7) · Claude Code");
  assert.equal(portUrl(5173), "http://localhost:5173");
  assert.match(stopPrompt(shell), /Stop vite \(pid 42\) listening on :5173\?/);
  assert.match(stopPrompt(shell), /shell that started it stays open/);
  assert.match(stopPrompt(harness), /harness that started it keeps running/);
});

test("globalPorts places a port wherever it lives, keeping every scope", () => {
  const terminals = [
    { ...term("t1", "w1", "term-1"), scope: "workstream" },
    { ...term("t2", "g9", "term-2"), scope: "goal" },
  ];
  const sessions = [
    { id: "s1", pid: 400, harness: "codex", goal: "g1", project: "p1", workstream: "w2" },
    { id: "s2", pid: 500, harness: "claude-code", goal: "g2", project: null, workstream: null },
  ];
  const ports = [
    tp(5173, 300, { kind: "terminal", terminal_id: "term-1" }, "vite"),
    tp(4000, 310, { kind: "terminal", terminal_id: "term-2" }, "node"),
    tp(3000, 410, { kind: "pid", pid: 400 }),
    tp(8080, 510, { kind: "pid", pid: 500 }),
  ];
  const placed = globalPorts(ports, terminals, sessions);
  assert.deepEqual(
    placed.map((p) => [p.port, p.owner.kind, p.owner.id]),
    [
      [3000, "workstream", "w2"],
      [4000, "goal", "g9"],
      [5173, "workstream", "w1"],
      [8080, "goal", "g2"],
    ],
    "sorted by port; a goal-scoped shell and a goal-only session keep their scope",
  );
  // The most specific place wins, and the rest rides along for a label.
  const ws = placed.find((p) => p.port === 3000);
  assert.equal(ws.owner.via, "harness");
  assert.equal(ws.owner.project, "p1");
  assert.equal(ws.owner.goal, "g1");
});

test("globalPorts drops an orphan and dedupes a dual-stack bind", () => {
  const placed = globalPorts(
    [tp(9000, 1, { kind: "pid", pid: 999 }), tp(5173, 300, { kind: "terminal", terminal_id: "term-1" }), tp(5173, 300, { kind: "terminal", terminal_id: "term-1" })],
    [{ ...term("t1", "w1", "term-1"), scope: "workstream" }],
    [],
  );
  assert.equal(placed.length, 1, "the orphan pid is dropped and the v4/v6 pair is one row");
  assert.equal(placed[0].port, 5173);
});

test("groupPorts buckets by owner in kind order", () => {
  const placed = [
    { port: 8080, pid: 2, process: "a", owner: { kind: "goal", id: "g1", via: "harness", harness: null } },
    { port: 5173, pid: 1, process: "b", owner: { kind: "workstream", id: "w1", via: "shell", harness: null } },
    { port: 5174, pid: 3, process: "c", owner: { kind: "workstream", id: "w1", via: "shell", harness: null } },
  ];
  const groups = groupPorts(placed);
  assert.deepEqual(groups.map((g) => `${g.kind}:${g.id}`), ["goal:g1", "workstream:w1"]);
  assert.deepEqual(groups[1].ports.map((p) => p.port), [5173, 5174]);
  assert.match(stopPlacedPrompt(placed[1]), /shell that started it stays open/);
  assert.match(stopPlacedPrompt(placed[0]), /harness that started it keeps running/);
});

test("the ports summary counts the ports and where they were started; nothing for none", () => {
  const shell = (port) => ({ port, pid: port, process: "vite", owner: { kind: "workstream", id: "w1", harness: null, via: "shell" } });
  const harness = (port) => ({ port, pid: port, process: "node", owner: { kind: "workstream", id: "w1", harness: "codex", via: "harness" } });
  assert.equal(portsSummary([shell(5173), shell(5174), harness(3000)]), "3 ports · 2 in terminals · 1 in a harness");
  assert.equal(portsSummary([shell(5173)]), "1 port · 1 in a terminal");
  assert.equal(portsSummary([harness(3000), harness(3001)]), "2 ports · 2 in harnesses");
  assert.equal(portsSummary([]), "");
});

test("a failed scan is said where the ports are read, and nothing is said while the scans answer", () => {
  assert.equal(portsKeptWords(null), "");
  assert.equal(portsKeptWords(undefined), "");
  assert.equal(portsKeptWords("   "), "");
  assert.equal(portsKeptWords("could not read the socket table"), "The ports could not be scanned — the last answer is kept: could not read the socket table");
});
