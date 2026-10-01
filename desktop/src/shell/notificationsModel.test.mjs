import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { CATEGORIES, MEMORY_CAP, NOTIFY_DEFAULTS, NOTIFY_KEYS, allowed, categoryOfState, emptyMemory, namesNotifyKey, onFrame, onTransition, readNotifyPrefs } from "./notificationsModel.mjs";

const row = (state, extra = {}) => ({
  id: "01L", kind: "worker", state, since: 1, harness: "claude-code", cost: { input_tokens: 0, output_tokens: 0, usd_cents: 0 }, children: [], last_activity: 1, ...extra,
});
const waiting = { state: "waiting", on: { on: "permission", tool: "Bash", gate_id: "g1" } };
const running = { state: "running", tool: "x", args: "" };

test("a session that starts waiting is an ask, said once, and its gate is remembered", () => {
  const first = onTransition({ state: "thinking" }, row(waiting, { agent: "dev" }), emptyMemory());
  assert.equal(first.notice.title, "Bisa — waiting on you");
  assert.equal(first.notice.category, "asks");
  assert.match(first.notice.body, /^dev: permission: Bash/);
  assert.deepEqual(first.memory.gates, ["g1"]);
  const again = onTransition(waiting, row(waiting), first.memory);
  assert.equal(again.notice, null, "still waiting is not news");
  const gate = onFrame({ type: "gate_opened", gate_id: "g1", gate: "escalation", question: "Allow Bash?" }, first.memory);
  assert.equal(gate.notice, null, "the gate the session announced is not announced twice");
  assert.equal(onTransition(null, row({ state: "thinking" }), emptyMemory()).notice, null, "a session appearing is not news");
});

test("a failed session is a failure and a finished one is finished work; the model names the moment, never the person's wish", () => {
  const failed = onTransition(running, row({ state: "failed", reason: "boom" }), emptyMemory()).notice;
  assert.equal(failed.category, "failures");
  assert.match(failed.body, /boom/);
  const done = onTransition(running, row({ state: "done" }), emptyMemory()).notice;
  assert.deepEqual([done.category, done.title], ["done", "Bisa — done"]);
  assert.deepEqual(["waiting", "failed", "done", "thinking"].map(categoryOfState), ["asks", "failures", "done", null]);
});

test("frames with no session behind them: a step's gate and a question are asks; a proposal and a stalled design are workflows", () => {
  const m = emptyMemory();
  const gate = onFrame({ type: "gate_opened", gate_id: "g9", gate: "approval", question: "Ship it?" }, m).notice;
  assert.deepEqual([gate.category, gate.body], ["asks", "Ship it?"]);
  const question = onFrame({ type: "question_asked", gate_id: "g8", text: "Which?", expects: "decision" }, m).notice;
  assert.deepEqual([question.category, question.title], ["asks", "Bisa — a question for you"]);
  const proposal = onFrame({ type: "workflow_proposed", workflow: "w", revision: 1, gate_id: "g" }, m).notice;
  assert.deepEqual([proposal.category, proposal.title], ["workflows", "Bisa — a workflow to adopt"]);
  const stalled = onFrame({ type: "guided", phase: "design", status: "stalled", detail: "timed out" }, m).notice;
  assert.equal(stalled.category, "workflows");
  assert.match(stalled.body, /timed out/);
  assert.equal(onFrame({ type: "guided", phase: "design", status: "working" }, m).notice, null);
  assert.equal(onFrame({ type: "step_changed" }, m).notice, null);
  assert.equal(onFrame(null, m).notice, null);
});

test("the gate memory is bounded", () => {
  let m = emptyMemory();
  for (let i = 0; i < MEMORY_CAP + 5; i++) {
    m = onFrame({ type: "gate_opened", gate_id: `g${i}`, gate: "approval", question: "?" }, m).memory;
  }
  assert.equal(m.gates.length, MEMORY_CAP);
  assert.equal(m.gates[0], "g5", "the oldest went");
});

test("the committer ask reaches the OS only when nobody was there to see it, as an ask", () => {
  const m = emptyMemory();
  const ask = (reason, origin) => ({ type: "committer_needed", project: "P", slug: "web", workstream: "P", reason, origin: { origin }, global: null });
  const refused = onFrame(ask("settlement_refused", "goal"), m).notice;
  assert.deepEqual([refused.category, refused.title], ["asks", "Bisa — who commits?"]);
  assert.match(refused.body, /kept uncommitted/);
  assert.match(onFrame(ask("created", "step"), m).notice.body, /workflow step/);
  assert.equal(onFrame(ask("created", "workspace"), m).notice, null, "a person clicking create sees the dialog instead");
  assert.equal(onFrame(ask("created", "goal"), m).notice, null);
  assert.equal(onFrame(ask("commit_refused", "workspace"), m).notice, null, "the person who pressed commit is looking at the refusal");
  assert.equal(onFrame({ type: "committer_set", project: "P", workstream: "P", identity: { name: "A", email: "a@b.c" } }, m).notice, null);
});

test("a start event that could not start a run and a failed workstream script reach the OS once, as failures", () => {
  const failed = { type: "listener_failed", listener: "workspace:01J0WF/ticket", signal: "s1", error: "the hook body was not JSON" };
  const first = onFrame(failed, emptyMemory());
  assert.equal(first.notice.category, "failures");
  assert.equal(first.notice.title, "Bisa — a start event failed");
  assert.equal(first.notice.body, "A workflow's start event could not start a run: the hook body was not JSON");
  assert.equal(onFrame(failed, first.memory).notice, null, "the same event is announced once");
  assert.notEqual(onFrame({ ...failed, signal: "s2" }, first.memory).notice, null, "the next event that fails is news");
  const goal = onFrame({ type: "listener_failed", listener: "goal:01J0G/ticket", error: "the goal's budget is spent" }, emptyMemory()).notice;
  assert.equal(goal.body, "A goal's start event could not start a run: the goal's budget is spent", "a listening goal's start event, by its listener");
  assert.equal(onFrame({ type: "listener_failed", listener: "workspace:01J0WF/ticket" }, emptyMemory()).notice.body, "A workflow's start event could not start a run.", "no error: the sentence alone");
  assert.equal(onFrame({ type: "trigger_failed", trigger: "t1", signal: "s1", error: "x" }, emptyMemory()).notice, null, "a retired frame is nobody's news");

  const script = { type: "workstream_script_ran", workstream: "w1", project: "p1", phase: "clean", ok: false, output: "failed: exit 2\nrm: cannot" };
  const ran = onFrame(script, emptyMemory());
  assert.equal(ran.notice.category, "failures");
  assert.match(ran.notice.title, /script failed/);
  assert.equal(ran.notice.body, "failed: exit 2", "the first line of what it printed");
  assert.equal(onFrame(script, ran.memory).notice, null);
  assert.equal(onFrame({ ...script, ok: true }, emptyMemory()).notice, null, "a script that passed is not a failure");

  // What the person approved and did not go out: said once, with the act and why.
  const unsent = { type: "workstream_publish_failed", workstream: "w1", project: "p1", what: "push fix/total", reason: "no usable remote: the remote hung up\nfatal: …" };
  const told = onFrame(unsent, emptyMemory());
  assert.equal(told.notice.category, "failures");
  assert.equal(told.notice.title, "Bisa — what you approved did not go out");
  assert.equal(told.notice.body, "Could not push fix/total: no usable remote: the remote hung up");
  assert.equal(onFrame(unsent, told.memory).notice, null, "the same end is not announced twice");
  assert.equal(onFrame({ ...unsent, reason: "" }, emptyMemory()).notice.body, "Could not push fix/total.");
});

test("the person's switches: one master over everything — the app's own word included — then one per category, finished work off by default", () => {
  const defaults = readNotifyPrefs([]);
  assert.deepEqual(defaults, NOTIFY_DEFAULTS);
  assert.deepEqual([...CATEGORIES], ["asks", "failures", "done", "workflows", "addons"]);
  for (const c of ["asks", "failures", "workflows", "addons", "app"]) assert.equal(allowed(defaults, c), true, `${c} is on by default`);
  assert.equal(allowed(defaults, "done"), false, "finished work is not an interruption until asked for");
  const wantDone = readNotifyPrefs([{ key: NOTIFY_KEYS.done, value: true }]);
  assert.equal(allowed(wantDone, "done"), true);
  const muted = readNotifyPrefs([{ key: NOTIFY_KEYS.enabled, value: false }, { key: NOTIFY_KEYS.done, value: true }]);
  for (const c of ["asks", "failures", "done", "workflows", "addons", "app"]) assert.equal(allowed(muted, c), false, `the master off keeps ${c} back`);
  const noAsks = readNotifyPrefs([{ key: NOTIFY_KEYS.asks, value: false }, { key: NOTIFY_KEYS.workflows, value: false }]);
  assert.equal(allowed(noAsks, "asks"), false);
  assert.equal(allowed(noAsks, "workflows"), false, "a proposal and a stalled design mute together");
  assert.equal(allowed(noAsks, "failures"), true);
  assert.equal(allowed(noAsks, "app"), true, "the app's own word has no switch but the master");
  assert.equal(allowed(defaults, "nonsense"), false, "an unknown category never reaches the OS");
  assert.equal(readNotifyPrefs([{ key: NOTIFY_KEYS.asks, value: "yes" }]).asks, true, "a value that is not a boolean is the default");
  assert.equal(namesNotifyKey([NOTIFY_KEYS.addons]), true);
  assert.equal(namesNotifyKey(["desktop.dock_icon"]), false);
  assert.equal(namesNotifyKey(null), false);
});

test("every switch is a registry key of the notifications group, and nothing names the old agents.notify keys", () => {
  const settings = readFileSync(new URL("../../../crates/bisa-core/src/settings.rs", import.meta.url), "utf8");
  for (const key of Object.values(NOTIFY_KEYS)) {
    assert.ok(key.startsWith("notifications."), key);
    assert.ok(settings.includes(`"${key}",`), `${key} is registered`);
  }
  assert.ok(!settings.includes("agents.notify"), "the old keys are gone, not aliased");
});

test("a notice's body says who and what in the wait's own words — never a label with its first words cut off", () => {
  const body = (state, extra) => onTransition({ state: "thinking" }, row(state, extra), emptyMemory()).notice.body;
  assert.equal(body({ state: "waiting", on: { on: "permission", tool: "Bash", gate_id: "g1" } }, { agent: "dev" }), "dev: permission: Bash");
  assert.equal(body({ state: "waiting", on: { on: "question", text: "Which database?", gate_id: "g2" } }, { agent: "dev" }), "dev: Which database?");
  assert.equal(body({ state: "waiting", on: { on: "gate", gate: "approval", gate_id: "g3" } }), "claude-code: approval gate", "with no agent named, the harness");
  assert.equal(body({ state: "waiting", on: { on: "auth", provider: "GitHub" } }), "claude-code: sign in to GitHub");
  assert.equal(body({ state: "waiting" }, { agent: "dev" }), "dev", "a wait that says no more than that it waits: the title said so already");
  const failed = (state) => onTransition(running, row(state, { agent: "dev" }), emptyMemory()).notice.body;
  assert.equal(failed({ state: "failed", reason: "the model refused" }), "dev: the model refused");
  assert.equal(failed({ state: "failed" }), "dev: failed", "a failure with no reason still says it failed");
  assert.equal(failed({ state: "failed", reason: "" }), "dev: failed");
  const model = readFileSync(new URL("./notificationsModel.mjs", import.meta.url), "utf8");
  assert.ok(!model.includes(".replace(/^waiting") && !model.includes('?? "failed"'), "no English is cut or spelt in the model: a translation would break the one and miss the other");
});
