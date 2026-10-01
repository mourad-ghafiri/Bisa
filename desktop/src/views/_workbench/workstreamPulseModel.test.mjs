/**
 * The rail's pulse line as facts. Run with `node --test desktop/src/views/_workbench/workstreamPulseModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { ARGS_CHARS, headlineOf, moreLabel, projectPulse, pulseOf, truncate } from "./workstreamPulseModel.mjs";

const NOW = 10_000;
const S = (id, state, extra = {}) => ({
  id,
  kind: "worker",
  state,
  since: NOW - 12,
  harness: "claude-code",
  agent: null,
  work_item: null,
  workstream: "w1",
  project: "p1",
  goal: null,
  cost: { input_tokens: 0, output_tokens: 0, usd_cents: 0 },
  children: [],
  last_activity: NOW - 1,
  ...extra,
});
const running = (tool, args = "", tier = "read") => ({ state: "running", tool, args, tier });
const permission = (tool, gate_id = "g1") => ({ state: "waiting", on: { on: "permission", tool, gate_id } });
const shell = (id, code) => ({ key: `t${id}`, scope: "workstream", id, harness: null, label: null, resume: false, generation: 0, liveness: { status: "exited", code }, restoring: false });
const liveShell = (id, harness = null, busy = false) => ({ key: `t${id}`, scope: "workstream", id, harness, label: null, resume: false, generation: 0, liveness: { status: "live" }, restoring: false, busy });
const port = (p, workstream = "w1") => ({ port: p, pid: 1, process: "vite", workstream, via: { kind: "shell", key: "t1", harness: null } });
const pulse = (sessions, terminals = [], workstream = "w1", now = NOW, ports = []) => pulseOf({ sessions, terminals, workstream, now, ports });

test("nothing standing here is no line at all", () => {
  assert.equal(pulse([]), null);
  assert.equal(pulse([S("s1", running("Read"), { workstream: "elsewhere" })]), null, "another workstream's session is not this row's");
  assert.equal(pulse([], [shell("w1", 0)]), null, "a shell that exited cleanly is not news");
});

test("every state has a headline, and the tool's arguments and a reason are cut to one line", () => {
  assert.equal(headlineOf(running("Edit", "src/cart.rs")), "running Edit · src/cart.rs");
  assert.equal(headlineOf(running("Edit")), "running Edit");
  const long = "a".repeat(ARGS_CHARS + 20);
  assert.equal(headlineOf(running("Bash", long)), `running Bash · ${"a".repeat(ARGS_CHARS)}…`);
  assert.equal(headlineOf(permission("Bash")), "waiting on you — permission: Bash");
  assert.equal(headlineOf({ state: "waiting", on: { on: "question", text: "Which one?", gate_id: "g" } }), "waiting on you — Which one?");
  assert.equal(headlineOf({ state: "failed", reason: "x".repeat(80) }), `failed: ${"x".repeat(60)}…`);
  assert.equal(headlineOf({ state: "thinking" }), "thinking…");
  assert.equal(headlineOf({ state: "done" }), "done");
  assert.equal(headlineOf({ state: "starting" }), "starting");
  assert.equal(truncate("hello brave new world", 12), "hello brave…", "cut on a word when one is near");
  assert.equal(truncate("abcdefghijklmnop", 8), "abcdefgh…", "else on the character");
});

test("a running session: who, the words, the tier's glyph, how long, and Abort", () => {
  const p = pulse([S("s1", running("Edit", "src/cart.rs", "write"))]);
  assert.equal(p.word, "running");
  assert.equal(p.tone, "working");
  assert.equal(p.tier, "write");
  assert.equal(p.who, "claude-code", "the harness when no agent persona runs");
  assert.equal(p.headline, "running Edit · src/cart.rs");
  assert.equal(p.elapsed, "12s", "a live state counts up in seconds — never 'just now'");
  assert.deepEqual(p.cta, { kind: "abort", sessionId: "s1" });
  assert.equal(p.more, null);
  assert.equal(p.subagents.total, 0);
  assert.equal(pulse([S("s1", running("Read"), { agent: "dev" })]).who, "dev", "the agent persona when one runs");
  const idle = pulse([S("s1", { state: "idle" })]);
  assert.equal(idle.word, "idle");
  assert.equal(idle.cta, null, "a session idle between turns is alive but nothing runs — no Abort");
  assert.deepEqual(pulse([S("s1", { state: "starting" })]).cta, { kind: "abort", sessionId: "s1" }, "a session that never spoke can still be stopped");
});

test("liveStart is the second a live counter counts from, and null once ended", () => {
  // A live session: the view ticks `liveStart` in a leaf instead of the baked
  // `elapsed`, so a running rail no longer rebuilds every few seconds.
  const live = pulse([S("s1", running("Read"), { started: NOW - 20, since: NOW - 12 })]);
  assert.equal(live.liveStart, NOW - 20, "the run's start, not the state instant");
  assert.equal(live.elapsed, "20s", "the baked string stays for callers that do not tick it");

  const ended = pulse([S("s1", { state: "done" }, { since: NOW - 60 })]);
  assert.equal(ended.liveStart, null, "an ended line does not count up");

  const shellUp = pulse([], [{ ...liveShell("w1"), openedAt: NOW - 30 }]);
  assert.equal(shellUp.liveStart, NOW - 30, "a live shell counts from when it opened");
  assert.equal(pulse([], [liveShell("w1")]).liveStart, null, "a shell with no opened-at has nothing to count");
});

test("waiting on you leads everything else and offers Answer when a gate exists", () => {
  const p = pulse([S("s1", running("Read")), S("s2", permission("Bash", "g9"))]);
  assert.equal(p.word, "waiting");
  assert.equal(p.tone, "accent");
  assert.equal(p.headline, "waiting on you — permission: Bash");
  assert.deepEqual(p.cta, { kind: "answer", gateId: "g9" });
  assert.deepEqual(p.more, { count: 1, word: "working" });
  assert.equal(moreLabel(p.more), "+1 working");
  const auth = pulse([S("s1", { state: "waiting", on: { on: "auth", provider: "GitHub" } })]);
  assert.equal(auth.headline, "waiting on you — sign in to GitHub");
  assert.deepEqual(auth.cta, { kind: "abort", sessionId: "s1" }, "an auth wait has no gate to answer; the session can still be aborted");
});

test("a failure is red with its reason, and an ended session reads how long ago — then leaves", () => {
  const p = pulse([S("s1", { state: "failed", reason: "boom" }, { since: NOW - 3 * 60 })]);
  assert.equal(p.tone, "danger");
  assert.equal(p.headline, "failed: boom");
  assert.equal(p.elapsed, "3m", "an ended state reads as words about the past");
  assert.equal(p.cta, null, "nothing to abort once it ended");
  const done = pulse([S("s1", { state: "done" }, { since: NOW - 60 })]);
  assert.equal(done.tone, "ok");
  assert.equal(done.headline, "done");
  assert.equal(pulse([S("s1", { state: "done" }, { since: NOW - 16 * 60 })]), null, "a session that ended a quarter of an hour ago says nothing");
  assert.notEqual(pulse([S("s1", running("Read"), { since: NOW - 16 * 60 })]), null, "a live one never expires");
});

test("a sub-agent leads only when it is louder than its parent, and the chip counts them", () => {
  const kids = [
    { id: "c1", name: "explore", description: "map the crate", state: running("Grep", "fn main"), since: NOW - 5 },
    { id: "c2", name: "tester", description: "run the suite", state: { state: "thinking" }, since: NOW - 4 },
  ];
  const calm = pulse([S("s1", running("Read"), { children: kids })]);
  assert.equal(calm.who, "claude-code", "a running parent is not upstaged by a running child");
  assert.deepEqual(calm.subagents, { total: 2, working: 2, waiting: 0, failed: 0, names: ["explore — running Grep", "tester — thinking"] });
  const loud = pulse([S("s1", running("Read"), { children: [{ ...kids[0], state: permission("Bash", "g3") }, kids[1]] })]);
  assert.equal(loud.who, "↳ explore", "a waiting sub-agent is the subject");
  assert.equal(loud.headline, "waiting on you — permission: Bash");
  assert.deepEqual(loud.cta, { kind: "answer", gateId: "g3" });
  assert.equal(loud.subagents.total, 0, "the chip is the parent's; a sub-agent has none");
  assert.equal(loud.more, null, "the parent is not 'one more' — it is the subject's own session");
  const failedKid = pulse([S("s1", running("Read"), { children: [{ ...kids[0], state: { state: "failed", reason: "the sub-agent failed" } }] })]);
  assert.equal(failedKid.who, "↳ explore");
  assert.equal(failedKid.tone, "danger");
});

test("several sessions fold into one more chip: the next-loudest word and its count", () => {
  const p = pulse([S("s1", running("Read"), { last_activity: NOW }), S("s2", running("Edit"), { last_activity: NOW - 9 }), S("s3", { state: "thinking" }, { last_activity: NOW - 8 })]);
  assert.equal(p.who, "claude-code");
  assert.equal(p.headline, "running Read", "ties break by the newest activity");
  assert.deepEqual(p.more, { count: 2, word: "working" }, "the rest, by the next-loudest word — a live one reads as working");
  const thinker = pulse([S("s1", { state: "thinking" }, { last_activity: NOW }), S("s2", running("Edit"), { last_activity: NOW - 9 })]);
  assert.equal(thinker.headline, "running Edit", "a named tool is the more telling line, whatever the dot's order says");
  const finished = pulse([S("s1", { state: "done" }, { last_activity: NOW }), S("s2", running("Edit"), { last_activity: NOW - 9 })]);
  assert.equal(finished.headline, "running Edit", "a live session outranks one that finished");
  const failed = pulse([S("s1", running("Read")), S("s2", { state: "failed", reason: "a" }), S("s3", { state: "failed", reason: "b" })]);
  assert.equal(failed.word, "failed");
  assert.deepEqual(failed.more, { count: 1, word: "failed" });
});

test("a shell that exited badly speaks only when no agent is here", () => {
  const p = pulse([], [shell("w1", 137)]);
  assert.equal(p.word, "failed");
  assert.equal(p.headline, "failed: the shell exited (137)");
  assert.equal(p.who, "shell");
  assert.equal(p.elapsed, "", "a shell has no since");
  assert.equal(p.cta, null);
  assert.equal(pulse([S("s1", running("Read"))], [shell("w1", 137)]).who, "claude-code", "an agent outranks a dead shell in the line; the dot still counts it");
});

test("a live shell alone gets a quiet line so its ports have a home; it never reads running", () => {
  const bare = pulse([], [liveShell("w1")]);
  assert.equal(bare.word, "idle", "a shell says nothing about what runs in it");
  assert.equal(bare.tone, "dim");
  assert.equal(bare.headline, "shell open");
  assert.equal(bare.who, "shell");
  assert.equal(bare.elapsed, "", "a shell has no since");
  assert.deepEqual(bare.ports, []);
  assert.equal(pulse([], [{ ...liveShell("w1"), busy: true }]).word, "idle", "bytes on the PTY are not a state");
  assert.equal(pulse([], [liveShell("w1"), liveShell("w1")]).headline, "2 shells open");
  assert.equal(pulse([], [liveShell("w1", "claude-code")]).who, "claude-code", "the harness names the line when one runs in the shell");
  assert.equal(pulse([], []), null, "no shell, no line");
});

test("the workstream's ports ride the line, whoever leads it", () => {
  const onAgent = pulse([S("s1", running("Read"))], [], "w1", NOW, [port(5173)]);
  assert.deepEqual(onAgent.ports.map((p) => p.port), [5173], "a harness that opened a port shows it on its own line");
  const onShell = pulse([], [liveShell("w1")], "w1", NOW, [port(3000)]);
  assert.deepEqual(onShell.ports.map((p) => p.port), [3000], "and a bare shell carries its own");
});

test("the visible time is the total while live and the run's length once done", () => {
  // Entered `waiting` 5s ago but running for 20m: the line shows the total, and
  // the current-state timer moves to the tooltip.
  const live = pulse([S("s1", permission("Bash"), { since: NOW - 5, started: NOW - 1200 })]);
  assert.equal(live.elapsed, "20m", "total open time, not the 5s in state");
  assert.match(live.detail, /in this state 5s/, "the state timer is on hover");
  // Done: how long it took, start → the done instant, to the second.
  const done = pulse([S("s1", { state: "done" }, { since: NOW - 30, started: NOW - 30 - (2 * 60 + 14) })]);
  assert.equal(done.elapsed, "2m 14s", "how long the run took, precisely");
  // A live shell reads how long it has been open.
  const shell = pulse([], [{ ...liveShell("w1"), openedAt: NOW - 12 * 60 }]);
  assert.equal(shell.elapsed, "12m");
  // No start recorded (a sub-agent, an old session): a graceful fallback.
  assert.equal(pulse([S("s1", running("Read"), { since: NOW - 8 })]).elapsed, "8s", "start falls back to since");
});

test("the flash key changes on a visible transition and not on a token", () => {
  const a = pulse([S("s1", running("Read"), { since: 100, last_activity: 200 })]);
  const b = pulse([S("s1", running("Read"), { since: 100, last_activity: 300 })]);
  assert.equal(a.flashKey, b.flashKey, "a newer last_activity alone is a token: no flash");
  const c = pulse([S("s1", running("Edit"), { since: 101, last_activity: 300 })]);
  assert.notEqual(a.flashKey, c.flashKey, "a new state at a new since flashes");
});

test("a collapsed project takes the loudest of its workstreams' lines", () => {
  const quiet = pulse([S("s1", running("Read"), { since: NOW - 50 })]);
  const loud = pulse([S("s2", permission("Bash"), { workstream: "w2", since: NOW - 5 })], [], "w2");
  assert.equal(projectPulse([quiet, null, loud]), loud);
  assert.equal(projectPulse([null, null]), null);
  const older = pulse([S("s3", running("Grep"), { workstream: "w3", since: NOW - 90 })], [], "w3");
  assert.equal(projectPulse([older, quiet]), quiet, "equally loud: the newest state wins");
});

test("the pulse's sub-agents are the live ones, counted from their own start, and an exited tab ends the lead", () => {
  const children = [
    { id: "c1", name: "explore", description: "", state: { state: "running", tool: "Read", args: "", tier: "read" }, since: NOW - 5, started: NOW - 40 },
    { id: "c2", name: "plan", description: "", state: { state: "done" }, since: NOW - 3, started: NOW - 30 },
  ];
  const p = pulse([S("s1", running("sub-agent", "explore"), { children })]);
  assert.equal(p.subagents.total, 1, "a finished sub-agent is not counted");
  assert.deepEqual(p.subagents.names, ["explore — running Read"]);
  // A sub-agent that leads (it asks) counts from its spawn, not its state.
  const asking = [{ id: "c9", name: "review", description: "", state: permission("Bash"), since: NOW - 2, started: NOW - 20 }];
  const lead = pulse([S("s1", running("sub-agent", "review"), { children: asking })]);
  assert.equal(lead.who, "↳ review");
  assert.equal(lead.elapsed, "20s", "since it was spawned");
  assert.equal(lead.liveStart, NOW - 20);
  // The claiming tab exited: the lead is over as of the exit, its children gone.
  const tab = { ...liveShell("w1", "claude-code"), key: "t1", sessionId: "s1", liveness: { status: "exited", code: 0 }, exitedAt: NOW - 10 };
  const over = pulse([S("s1", running("Edit"), { kind: "terminal", children })], [tab]);
  assert.equal(over.word, "done");
  assert.equal(over.since, NOW - 10);
  assert.equal(over.subagents.total, 0);
  assert.equal(over.liveStart, null, "nothing counts up");
});
