/**
 * What the activity *says*, tested.
 *
 * This module had no test at all, which is how six `EnginePayload` variants
 * spent a milestone with no `switch` arm while the CLI rendered every one of
 * them. So the tests that matter most here are the ones that walk a whole
 * union: a variant that renders nothing must be named as a deliberate
 * omission, and a variant nobody thought about must fail.
 *
 * Run with `npm test` from `desktop/`.
 */

import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import {
  appendCapped,
  cents,
  conversationLine,
  engineLine,
  guidedWords,
  hostOf,
  journalLine,
  payloadLine,
  pulseLine,
  sessionLine,
  shortId,
  truncate,
} from "./activityModel.mjs";

const WORK_ITEM = "01J000000000000000WORK1";
const SESSION = "01J000000000000000SESS1";
const RUN = "01J0000000000000000RUN1";
const WORKFLOW = "01J00000000000000000WF1";

/** One of every `JournalPayload` variant, keyed by its wire tag. */
const JOURNAL = {
  note: { type: "note", text: "the total is wrong on the invoice" },
  guidance: { type: "guidance", phase: "design", status: "stalled", detail: "no proposal after 600s", session: SESSION },
  guard: {
    type: "guard",
    tool: "Bash",
    subject: "curl -H 'Authorization: Bearer «secret:bearer:0a1b2c»' https://api.example.test",
    verdict: "asked",
    by: "classifier",
    rule: "network_and_remote",
    reason: "it sends a credential to a remote host",
  },
  judgement: {
    type: "judgement",
    run: RUN,
    step: "assign",
    judgement: {
      point: "assign.pick",
      provider: "jev",
      model: "jev-latest",
      calibrated: true,
      questions: {},
      answers: { pick: { type: "choice", choice: "developer", probabilities: { developer: 0.91, tester: 0.09 }, confidence: 0.91 } },
      outcome: "applied",
      latency_ms: 120,
      usage: { input_tokens: 40, output_tokens: 8 },
    },
  },
  step: {
    type: "step",
    run: RUN,
    step: "implement",
    event: { fact: "failed", error: "the tests did not pass" },
  },
  run: {
    type: "run",
    run: RUN,
    event: { fact: "started", workflow: WORKFLOW, revision: 2 },
  },
  decision: {
    type: "decision",
    gate: "approval",
    approve: true,
    subject: "adopt:01J0WF@2",
    rationale: "scope looks right",
    answer: { selected: ["ship"], text: "yes, ship it", unsure: false },
  },
  question: {
    type: "question",
    work_item: WORK_ITEM,
    gate: "gate-7",
    text: "which currency should the total use?",
    expects: { kind: "answer", options: [{ id: "eur", label: "EUR" }, { id: "usd", label: "USD" }] },
  },
  withdrawn: { type: "withdrawn", subject: "workstream:01J0WS", reason: "interrupted by a restart" },
  claim: { type: "claim", work_item: WORK_ITEM, harness: "claude-code", session: SESSION },
  progress: {
    type: "progress",
    work_item: WORK_ITEM,
    verb: "edited",
    object: "src/total.rs",
    outcome: "tests pass",
  },
  result: {
    type: "result",
    work_item: WORK_ITEM,
    output: { total: 42 },
    artifacts: ["branch:fix/total", "commit:abcdef"],
  },
  attachment: { type: "attachment", project: "01J00000000000000000PR1", attached: true },
  document: {
    type: "document",
    file: { sha256: "ab".repeat(32), name: "brief.pdf", mime: "application/pdf", size: 1234 },
  },
  signal: {
    type: "signal",
    signal: "01J00000000000000SIG01",
    listener: `workspace:${WORKFLOW}/ticket`,
    source: "hook",
    payload: { body: "the total is wrong on the invoice" },
  },
  turn_metrics: {
    type: "turn_metrics",
    session: SESSION,
    input_tokens: 1200,
    output_tokens: 340,
    usd_cents: 250,
  },
};

// ---------------------------------------------------------------------------
// Journal payloads
// ---------------------------------------------------------------------------

test("the journal's fixture holds one of every payload the core writes", () => {
  const rust = readFileSync(new URL("../../crates/bisa-core/src/event.rs", import.meta.url), "utf8");
  const start = rust.indexOf("pub enum JournalPayload {");
  assert.ok(start >= 0, "the core declares its journal payloads");
  const body = rust.slice(start, rust.indexOf("\n}", start));
  const variants = [...body.matchAll(/^ {4}([A-Z][A-Za-z0-9]*)\s*(?:\{|\(|,)/gm)].map((m) => m[1].replace(/([a-z0-9])([A-Z])/g, "$1_$2").toLowerCase());
  assert.ok(variants.length > 0, "the enum was read");
  assert.deepEqual(Object.keys(JOURNAL).sort(), [...variants].sort(), "a payload the core writes and this file never words is a line nobody read");
  for (const [tag, payload] of Object.entries(JOURNAL)) assert.equal(payload.type, tag, `${tag} is keyed by its wire tag`);
});

test("a project joining or leaving a goal is a line on the spine that names the project", () => {
  const joined = payloadLine(JOURNAL.attachment);
  assert.equal(joined.tone, "spine");
  assert.equal(joined.icon, "icon:project");
  assert.match(joined.text, / attached$/);
  assert.deepEqual(joined.detail, [{ label: "Project", value: JOURNAL.attachment.project }]);
  assert.match(payloadLine({ ...JOURNAL.attachment, attached: false }).text, / detached$/);
});

test("every journal payload renders a line, and none renders nothing", () => {
  for (const [tag, payload] of Object.entries(JOURNAL)) {
    const line = payloadLine(payload);
    assert.ok(line, `${tag} produced no line`);
    assert.equal(typeof line.text, "string", `${tag} has no text`);
    assert.ok(line.text.length > 0, `${tag} rendered an empty line`);
    assert.ok(
      ["spine", "dim", "fail", "wait"].includes(line.tone),
      `${tag} has tone ${line.tone}, which is not one of the four`,
    );
  }
});

test("a payload the summary could not fit says so in detail rather than losing it", () => {
  // Every variant that carries more than its one line is expected to open —
  // a note included, whose whole text is the detail.
  const withDetail = Object.entries(JOURNAL)
    .filter(([, p]) => payloadLine(p).detail)
    .map(([tag]) => tag);
  assert.deepEqual(withDetail.sort(), Object.keys(JOURNAL).sort());
});

test("a withdrawn question says why and reads dim, not as a failure", () => {
  const line = payloadLine(JOURNAL.withdrawn);
  assert.equal(line.tone, "dim");
  assert.match(line.text, /withdrew a question — interrupted by a restart/);
  assert.ok(line.detail.some((d) => d.label === "Subject" && d.value === "workstream:01J0WS"));
});

test("turn metrics reach the activity at all", () => {
  // The Pulse route used to return `None` for this variant, so tokens and
  // cost were the one fact the CLI printed and the desktop could not show.
  const line = payloadLine(JOURNAL.turn_metrics);
  assert.equal(line.text, "1540 tokens · $2.50");
  assert.equal(line.icon, "icon:cost");
});

test("a failed step says why on the line, and carries the rest", () => {
  const line = payloadLine(JOURNAL.step);
  assert.equal(line.tone, "fail");
  assert.equal(line.icon, "step:failed");
  assert.equal(line.text, "step implement failed: the tests did not pass");
  assert.deepEqual(line.detail.find((f) => f.label === "Error"), {
    label: "Error",
    value: "the tests did not pass",
  });
  assert.deepEqual(line.detail.find((f) => f.label === "Run"), { label: "Run", value: RUN });
});

test("every step fact renders with its own state glyph, and a branch is named", () => {
  const facts = {
    started: ["dim", "step:running", /^step implement started/],
    waiting: ["wait", "step:waiting", /waiting on you$/],
    done: ["spine", "step:done", /^step implement done$/],
    skipped: ["dim", "step:skipped", /skipped$/],
    cancelled: ["dim", "step:cancelled", /cancelled$/],
  };
  for (const [fact, [tone, icon, text]] of Object.entries(facts)) {
    const line = payloadLine({ type: "step", run: RUN, step: "implement", event: { fact } });
    assert.equal(line.tone, tone, fact);
    assert.equal(line.icon, icon, fact);
    assert.match(line.text, text, fact);
  }
  const branched = payloadLine({ type: "step", run: RUN, step: "verdict", event: { fact: "done", branches: ["ship"] } });
  assert.equal(branched.text, "step verdict done → ship");
  assert.deepEqual(branched.detail.find((f) => f.label === "Branch"), { label: "Branch", value: "ship" });
  const every = payloadLine({ type: "step", run: RUN, step: "triage", event: { fact: "done", branches: ["urgent", "billing"] } });
  assert.equal(every.text, "step triage done → urgent, billing", "every rule that held names its branch");
  const retired = payloadLine({ type: "step", run: RUN, step: "verdict", event: { fact: "done", branch: "ship" } });
  assert.equal(retired.text, "step verdict done", "the retired single branch is not read");
  const answered = payloadLine({
    type: "step",
    run: RUN,
    step: "ask",
    event: { fact: "answered", answer: { selected: ["postgres"], text: null, unsure: false } },
  });
  assert.equal(answered.text, "answered step ask: chose postgres");
  const decided = payloadLine({ type: "step", run: RUN, step: "ship", event: { fact: "decided", approve: false, approval: "01A" } });
  assert.equal(decided.text, "declined step ship");
  assert.equal(decided.icon, "gate:approval");
});

test("a step a boundary event diverted is the run's story; one that acted beside the step is ambient", () => {
  const diverted = payloadLine({ type: "step", run: RUN, step: "review", event: { fact: "diverted", by: "late" } });
  assert.equal(diverted.tone, "spine");
  assert.equal(diverted.icon, "step:diverted");
  assert.equal(diverted.text, "step review diverted by boundary late");
  assert.deepEqual(diverted.detail.find((f) => f.label === "Boundary"), { label: "Boundary", value: "late" });
  const acted = payloadLine({ type: "step", run: RUN, step: "review", event: { fact: "boundary", boundary: "nudge" } });
  assert.equal(acted.tone, "dim");
  assert.equal(acted.text, "boundary nudge acted beside step review");
  // Live, a step's state moving to diverted says so before its fact names the boundary.
  const live = engineLine({ payload: { type: "step_changed", run: RUN, step: "review", state: "diverted", kind: "approval" } }, 1);
  assert.equal(live.text, "step review diverted");
  assert.equal(live.icon, "step:diverted");
});

test("a run started at a start names it, and the signal of the event that began it", () => {
  const started = payloadLine({ type: "run", run: RUN, event: { fact: "started", workflow: WORKFLOW, revision: 2, start: "nightly", signal: "01SIG" } });
  assert.deepEqual(started.detail.find((f) => f.label === "Step"), { label: "Step", value: "nightly" });
  assert.deepEqual(started.detail.find((f) => f.label === "Signal"), { label: "Signal", value: "01SIG" });
});

test("a run's facts read as the goal's story", () => {
  const started = payloadLine(JOURNAL.run);
  assert.equal(started.tone, "spine");
  assert.equal(started.icon, "icon:run");
  assert.equal(started.text, `run 00RUN1 started on workflow 000WF1 rev 2`);
  const done = payloadLine({ type: "run", run: RUN, event: { fact: "finished", outcome: "done" } });
  assert.equal(done.icon, "status:done");
  assert.equal(done.text, "run 00RUN1 finished done");
  const failed = payloadLine({ type: "run", run: RUN, event: { fact: "finished", outcome: "failed" } });
  assert.equal(failed.tone, "fail");
  assert.equal(failed.icon, "status:failed");
  const amended = payloadLine({ type: "run", run: RUN, event: { fact: "amended", revision: 3 } });
  assert.equal(amended.text, "run 00RUN1 amended to revision 3");
  const queued = payloadLine({ type: "run", run: RUN, event: { fact: "queued", workflow: WORKFLOW, revision: 2 } });
  assert.equal(queued.tone, "dim", "a queue is ambient");
  assert.equal(queued.icon, "icon:queued");
  assert.equal(queued.text, "run 00RUN1 queued on workflow 000WF1 rev 2");
  const stopped = payloadLine({
    type: "run",
    run: RUN,
    event: { fact: "cancelled", cause: { cause: "stopped", rationale: "enough" } },
  });
  assert.equal(stopped.tone, "dim", "a person's own act is never a failure");
  assert.equal(stopped.text, "run 00RUN1 stopped");
  assert.deepEqual(stopped.detail.find((f) => f.label === "Rationale"), { label: "Rationale", value: "enough" });
  const closed = payloadLine({
    type: "run",
    run: RUN,
    event: { fact: "cancelled", cause: { cause: "closed", reason: { reason: "abandoned", rationale: null } } },
  });
  assert.equal(closed.text, "run 00RUN1 closed");
  assert.deepEqual(closed.detail.find((f) => f.label === "Reason"), { label: "Reason", value: "abandoned" });
  for (const cause of ["restarted", "withdrawn"]) {
    assert.equal(payloadLine({ type: "run", run: RUN, event: { fact: "cancelled", cause: { cause } } }).text, `run 00RUN1 ${cause}`);
  }
});

test("a decision carries the subject, the rationale and what you answered", () => {
  const line = payloadLine(JOURNAL.decision);
  assert.equal(line.icon, "gate:approval");
  assert.match(line.text, /^approved the approval gate — chose ship — “yes, ship it”$/);
  assert.deepEqual(line.detail, [
    { label: "Subject", value: "adopt:01J0WF@2" },
    { label: "Rationale", value: "scope looks right" },
    { label: "Answer", value: "chose ship — “yes, ship it”" },
  ]);
});

test("a decision with nothing said still renders, without empty fields", () => {
  const line = payloadLine({
    type: "decision",
    gate: "escalation",
    approve: false,
    subject: "step:01J0RUN/ask",
  });
  assert.equal(line.text, "declined the escalation gate");
  assert.deepEqual(line.detail, [{ label: "Subject", value: "step:01J0RUN/ask" }]);
});

test("a decision-question and an answerable question do not read alike", () => {
  const answerable = payloadLine(JOURNAL.question).text;
  const decision = payloadLine({ ...JOURNAL.question, expects: { kind: "decision" } }).text;
  assert.notEqual(answerable, decision);
  assert.match(decision, /^asked for a decision: /);
  assert.match(answerable, /^asked “/);
});

test("a question says what kind it is, not what its expects object stringifies to", () => {
  // `expects` stopped being a string and became `{ kind }`. `String()` on one
  // is `[object Object]`, which is not a build error and so was never noticed.
  const line = payloadLine(JOURNAL.question);
  assert.deepEqual(
    line.detail.find((f) => f.label === "Expects"),
    { label: "Expects", value: "answer" },
  );
});

test("the options a question offered are part of what was asked", () => {
  const line = payloadLine(JOURNAL.question);
  assert.deepEqual(
    line.detail.find((f) => f.label === "Options"),
    { label: "Options", value: "EUR · USD" },
  );
});

test("a question with no options carries no empty options field", () => {
  const line = payloadLine({ ...JOURNAL.question, expects: { kind: "answer" } });
  assert.equal(
    line.detail.find((f) => f.label === "Options"),
    undefined,
  );
});

test("a recorded answer reads as words rather than as an object", () => {
  const line = payloadLine(JOURNAL.decision);
  assert.equal(line.text, "approved the approval gate — chose ship — “yes, ship it”");
  assert.doesNotMatch(line.text, /\[object/);
});

test("an unsure answer says so in the record, and does not read as a decline", () => {
  const line = payloadLine({
    ...JOURNAL.decision,
    answer: { selected: [], text: "ask the ops team", unsure: true },
  });
  assert.match(line.text, /^approved /);
  assert.match(line.text, /not sure — “ask the ops team”/);
});

test("a question carries the gate that would answer it", () => {
  const line = payloadLine(JOURNAL.question);
  assert.deepEqual(
    line.detail.find((f) => f.label === "Gate"),
    { label: "Gate", value: "gate-7" },
  );
});

test("a step or run line never carries Rust debug syntax", () => {
  // The server used to interpolate `{from:?}`, which put a Rust variant
  // spelling into a string the UI had no way to style. Ids are upper-case
  // ULIDs, so the check is on the words around them.
  for (const p of [JOURNAL.step, JOURNAL.run]) {
    const words = payloadLine(p).text.replace(/[0-9A-Z]{6,}/g, "");
    assert.doesNotMatch(words, /[A-Z][a-z]+::|\{|\}/);
  }
});

test("progress keeps the verb-object-outcome triple as three fields", () => {
  const line = payloadLine(JOURNAL.progress);
  assert.equal(line.text, "edited src/total.rs → tests pass");
  assert.deepEqual(line.detail, [
    { label: "Verb", value: "edited" },
    { label: "Object", value: "src/total.rs" },
    { label: "Outcome", value: "tests pass" },
    { label: "Work item", value: WORK_ITEM },
  ]);
});

test("a claim names the session, which is the handle on the transcript", () => {
  const line = payloadLine(JOURNAL.claim);
  assert.deepEqual(
    line.detail.find((f) => f.label === "Session"),
    { label: "Session", value: SESSION },
  );
});

test("a result carries its output and its artifacts", () => {
  const line = payloadLine(JOURNAL.result);
  assert.equal(line.text, "submitted the result for 0WORK1 (2 artifacts)");
  assert.equal(line.detail.find((f) => f.label === "Output").value, '{\n  "total": 42\n}');
  assert.equal(
    line.detail.find((f) => f.label === "Artifacts").value,
    "branch:fix/total\ncommit:abcdef",
  );
});

test("a payload this build has never heard of says its own name", () => {
  // A newer node writing into a shared workspace. An unnamed row is
  // indistinguishable from a bug in the renderer.
  const line = payloadLine({ type: "budget_exhausted" });
  assert.equal(line.text, "budget exhausted");
  assert.equal(line.tone, "dim");
});

test("a journal entry is its payload plus the envelope", () => {
  const line = journalLine({ home: { home: "goal", goal: "I1" }, author: "abc", at: 1000, payload: JOURNAL.note }, 3);
  assert.equal(line.key, "j:I1:1000:3");
  assert.equal(line.at, 1000);
  assert.equal(line.author, "abc");
  assert.equal(line.goal, "I1");
  assert.equal(line.text, payloadLine(JOURNAL.note).text);
});

test("an entry on a run of the workspace's journal names the run in its key and no goal", () => {
  const line = journalLine({ home: { home: "run", run: "R1" }, author: "abc", at: 1000, payload: JOURNAL.note }, 0);
  assert.equal(line.key, "j:R1:1000:0");
  assert.equal(line.goal, undefined);
  assert.equal(line.text, payloadLine(JOURNAL.note).text);
});

// ---------------------------------------------------------------------------
// Pulse rows
// ---------------------------------------------------------------------------

const stored = (event, over = {}) => ({
  seq: 7,
  at: 5,
  concept: "goals",
  kind: event.type,
  source: { kind: "goal", id: "I1" },
  title: "ship it",
  author: "abc",
  event,
  ...over,
});

test("a stored pulse row says exactly what the live line said", () => {
  // The point of shipping the event rather than a sentence: one renderer, so
  // an event cannot change appearance between arriving and being reloaded.
  for (const payload of Object.values(JOURNAL)) {
    const row = pulseLine(stored(payload));
    const live = journalLine({ home: { home: "goal", goal: "I1" }, author: "a", at: 5, payload }, 0);
    assert.equal(row.text, live.text);
    assert.equal(row.tone, live.tone);
    assert.equal(row.icon, live.icon);
  }
});

test("a pulse row carries its place, its concept, what it is about and its title as fields", () => {
  const row = pulseLine(stored(JOURNAL.note));
  assert.equal(row.key, "p:7", "the feed's own sequence is the key — unique across pages and reloads");
  assert.equal(row.goal, "I1", "a goal's row still names its conversation");
  assert.deepEqual(row.source, { kind: "goal", id: "I1" });
  assert.equal(row.concept, "goals");
  assert.equal(row.title, "ship it");
  assert.equal(row.author, "abc");
  // The title is a field, never spliced into the sentence — that is what lets
  // the view style it and collapse a run of rows under one heading.
  assert.doesNotMatch(row.text, /ship it/);
  assert.equal(pulseLine(stored(JOURNAL.note, { source: { kind: "channel", id: "C1" } })).goal, undefined, "only a goal's row is a goal's conversation");
});

test("a message and an engine fact stored in the feed render as themselves", () => {
  const message = pulseLine(stored({ type: "message", message_id: "m1", body_kind: "post", snippet: "hello everyone" }, { concept: "channels", source: { kind: "channel", id: "C1" }, title: "watercooler" }));
  assert.equal(message.text, "hello everyone");
  assert.equal(message.tone, "dim");
  assert.equal(message.icon, undefined);
  const joined = pulseLine(stored({ type: "message", message_id: "m2", body_kind: "membership", snippet: "" }, { concept: "channels" }));
  assert.equal(joined.text, "changed who is here", "a membership change with no words still says something happened");
  // An engine fact reads exactly as its frame did — one renderer, again.
  const live = engineLine({ payload: ENGINE.workstream_opened }, 5);
  const fact = pulseLine(stored(ENGINE.workstream_opened, { concept: "projects", source: { kind: "workstream", id: "W1" }, title: "feature/x" }));
  assert.equal(fact.text, live.text);
  assert.equal(fact.tone, live.tone);
  assert.equal(fact.icon, live.icon);
  assert.deepEqual(fact.detail, live.detail);
  assert.equal(fact.key, "p:7");
  const unknown = pulseLine(stored({ type: "invented_later" }, { concept: "node" }));
  assert.equal(unknown.text, "invented later", "a fact this build never heard of says its own name");
  // A note or a drawing changed has no live line, but the Workspace tab lists it: what changed and what it is about, never `note changed`.
  assert.equal(pulseLine(stored({ type: "note_changed", note: "N1", scope: "workspace" }, { concept: "workspace" })).text, "a workspace note changed");
  assert.equal(pulseLine(stored({ type: "drawing_changed", drawing: "D1", scope: "goal", hash: "h" }, { concept: "workspace" })).text, "a drawing about a goal changed");
  assert.equal(pulseLine(stored({ type: "note_changed", note: "N2", scope: "node" }, { concept: "workspace" })).text, "a note about this node changed");
  assert.equal(pulseLine(stored({ type: "drawing_changed", drawing: "D2", scope: "workspace", hash: "h" }, { concept: "workspace" })).icon, "icon:draw");
  // Under its subject's heading, a row does not say the subject again by its id; with no heading, the live words, id and all.
  assert.equal(pulseLine(stored({ type: "goal_created", goal: "G00001", origin: { origin: "captured" } }, { title: "Ship the report" })).text, "goal captured");
  assert.equal(pulseLine(stored({ type: "goal_created", goal: "G00001", origin: { origin: "run", run: RUN } }, { title: "Ship the report" })).text, "goal spawned by run 00RUN1");
  assert.equal(pulseLine(stored({ type: "agent_replied", scope: "I1", agent: "general-agent", posted: true }, { title: "General Agent" })).text, "replied");
  assert.equal(pulseLine(stored({ type: "agent_replied", scope: "I1", agent: "general-agent", posted: false }, { title: "General Agent" })).text, "acted without replying");
  assert.equal(pulseLine(stored({ type: "agent_replied", scope: "I1", agent: "general-agent", posted: true }, { title: undefined })).text, "general-agent replied");
  assert.equal(pulseLine(stored({ type: "goal_created", goal: "G00001" }, { title: null })).text, "goal G00001 captured");
});

// ---------------------------------------------------------------------------
// Live engine events
// ---------------------------------------------------------------------------

/** One of every `EnginePayload` variant except `session`, which delegates. */
const ENGINE = {
  scheduled: { type: "scheduled", harness: "claude-code" },
  execution_ended: { type: "execution_ended", outcome: { outcome: "completed" } },
  session_state: {
    type: "session_state",
    live_run: "01J0LIVE",
    presence: {
      id: "01J0LIVE",
      kind: "worker",
      state: { state: "waiting", on: { on: "permission", tool: "Bash", gate_id: "g1" } },
      since: 1,
      harness: "claude-code",
      cost: { input_tokens: 0, output_tokens: 0, usd_cents: 0 },
      children: [],
      last_activity: 1,
    },
  },
  session_gone: { type: "session_gone", live_run: "01J0LIVE" },
  gate_opened: { type: "gate_opened", gate_id: "g1", gate: "approval", question: "Adopt this workflow?" },
  question_asked: {
    type: "question_asked",
    gate_id: "g1",
    text: "which currency?",
    expects: { kind: "answer" },
  },
  guided: { type: "guided", phase: "design", status: "working", detail: "on claude-code", session: SESSION },
  gate_decided: { type: "gate_decided", gate_id: "g1", gate: "approval", approve: true },
  result_accepted: { type: "result_accepted" },
  run_started: { type: "run_started", run: RUN, workflow: WORKFLOW },
  run_queued: { type: "run_queued", run: RUN, workflow: WORKFLOW, position: 2 },
  run_finished: { type: "run_finished", run: RUN, outcome: "done" },
  run_cancelled: { type: "run_cancelled", run: RUN, cause: { cause: "stopped", rationale: null } },
  step_changed: { type: "step_changed", run: RUN, step: "implement", state: "running", kind: "agent" },
  goal_closed: { type: "goal_closed", reason: { reason: "abandoned", rationale: null } },
  goal_created: { type: "goal_created", goal: "01J0GOALCREATED", origin: { origin: "captured" } },
  workflow_proposed: { type: "workflow_proposed", workflow: WORKFLOW, revision: 1, gate_id: "g2" },
  workflow_changed: { type: "workflow_changed", workflow: WORKFLOW, revision: 2, designed: false },
  workflow_deleted: { type: "workflow_deleted", workflow: WORKFLOW },
  workflow_archived: { type: "workflow_archived", workflow: WORKFLOW, archived: true },
  attachment_changed: {
    type: "attachment_changed",
    project: "01J0PROJECT000000000000000",
    attached: true,
  },
  goal_archived: { type: "goal_archived", goal: "01J0GOALCREATED", archived: true },
  goal_deleted: { type: "goal_deleted", goal: "01J0GOALCREATED" },
  project_archived: { type: "project_archived", project: "01J0PROJECT000000000000000", archived: false },
  project_deleted: { type: "project_deleted", project: "01J0PROJECT000000000000000" },
  document_added: {
    type: "document_added",
    goal: "01J0GOALCREATED",
    file: { sha256: "ab".repeat(32), name: "brief.pdf", mime: "application/pdf", size: 1234 },
  },
  settings_changed: { type: "settings_changed", scope: "workspace", keys: ["editor.tab_size"] },
  conversation_created: { type: "conversation_created", id: "01J0CONVERSATION0000000000", origin: { kind: "goal", id: "01J0GOALCREATED" } },
  conversation_changed: { type: "conversation_changed", id: "01J0CONVERSATION0000000000", change: "renamed" },
  changes_moved: { type: "changes_moved", conversation: "01J0CONVERSATION0000000000", workstream: "W1", pending: 3 },
  changes_settled: { type: "changes_settled", conversation: "01J0CONVERSATION0000000000", act: "keep", files: 2, skipped: [] },
  ask_opened: {
    type: "ask_opened",
    conversation: "01J0CONVERSATION0000000000",
    ask: { id: "ask1", agent: "reviewer", tool: "Bash", tier: "exec", question: "Run `cargo clean`?", grantable: true, opened_at: 1 },
  },
  ask_settled: { type: "ask_settled", conversation: "01J0CONVERSATION0000000000", ask_id: "ask1", allowed: true },
  file_changed: {
    type: "file_changed",
    scope: "workstream",
    id: "01J0PROJECT000000000000000",
    path: "src/main.rs",
    kind: "modified",
    ignored: false,
  },
  lsp: {
    type: "lsp",
    scope: "workstream",
    id: "01J0PROJECT000000000000000",
    language: "rust",
    method: "bisa/serverStarted",
    params: { command: "rust-analyzer" },
  },
  agent_thinking: { type: "agent_thinking", scope: "I1", agent: "developer" },
  agent_replied: { type: "agent_replied", scope: "I1", agent: "developer", posted: true },
  workstream_opened: {
    type: "workstream_opened",
    workstream: "W1",
    project: "P1",
    path: "/w",
    branch: "fix/total",
    source: "a new branch",
  },
  workstream_changed:{ type: "workstream_changed", workstream: "W1", state: { state: "dirty_tree" } },
  workstream_edited: { type: "workstream_edited", workstream: "W1", project: "P1" },
  committer_needed: {
    type: "committer_needed",
    project: "P1",
    slug: "checkout",
    workstream: "P1",
    reason: "settlement_refused",
    origin: { origin: "step", goal: "G1", run: "R1", step: "build", workflow: "W1" },
    global: null,
  },
  committer_set: { type: "committer_set", project: "P1", workstream: "P1", identity: { name: "Ada", email: "ada@example.invalid" } },
  git_setup_changed: { type: "git_setup_changed", what: "profiles" },
  connectors_changed: { type: "connectors_changed", what: "accounts" },
  addons_changed: { type: "addons_changed", what: "enabled", id: "clock" },
  project_created: {
    type: "project_created",
    project: "P1",
    slug: "checkout",
    origin: { origin: "step", goal: "I1", run: RUN, step: "implement", workflow: WORKFLOW },
  },
  project_changed: { type: "project_changed", project: "P1" },
  workstream_committed: {
    type: "workstream_committed",
    workstream: "W1",
    branch: "fix/total",
    commit: "abcdef0123456789",
  },
  workstream_publish_failed: {
    type: "workstream_publish_failed",
    workstream: "W1",
    project: "P1",
    what: "push fix/total",
    reason: "no usable remote: the remote hung up\nfatal: could not read from remote repository",
  },
  workstream_script_ran: {
    type: "workstream_script_ran",
    workstream: "W1",
    project: "P1",
    phase: "post_create",
    ok: false,
    output: "exited with status 2\nnpm ERR! missing script: build",
  },
  guard_decided: {
    type: "guard_decided",
    session: SESSION,
    tool: "Bash",
    subject: "sudo make install",
    verdict: "denied",
    by: "rule",
    rule: "privilege_escalation",
    reason: "refused by the guard rule “sudo, doas, su”",
  },
  judged: {
    type: "judged",
    judgement: {
      point: "assign.pick",
      provider: "jev",
      model: "jev-latest",
      calibrated: true,
      questions: {},
      answers: { pick: { type: "choice", choice: "developer", probabilities: { developer: 0.91, tester: 0.09 }, confidence: 0.91 } },
      outcome: "applied",
      latency_ms: 120,
      usage: { input_tokens: 40, output_tokens: 8 },
    },
  },
  redacted: { type: "redacted", count: 2, kinds: ["github_token", "env:MY_KEY"], at: "prompt" },
  mobile_development_changed: { type: "mobile_development_changed", what: "devices" },
  model_switched: {
    type: "model_switched",
    from: "big",
    to: "small",
    reason: "rate limited",
    retry_in_secs: 30,
    after_progress: true,
  },
  signal_received: { type: "signal_received", signal: "S1", listener: `workspace:${WORKFLOW}/ticket`, source: "hook" },
  listener_fired: { type: "listener_fired", listener: "goal:I1/ticket", signal: "S1", outcome: { outcome: "started", run: RUN, goal: "I1" } },
  listener_failed: { type: "listener_failed", listener: `workspace:${WORKFLOW}/ticket`, signal: "S1", error: "the inputs no longer bind: ticket" },
  listening_changed: { type: "listening_changed", host: `workspace:${WORKFLOW}`, on: true },
  boundary_fired: { type: "boundary_fired", run: RUN, workflow: WORKFLOW, step: "review", boundary: "late", diverts: true },
  paused: { type: "paused" },
  resumed: { type: "resumed" },
};

test("every engine payload reaches the activity", () => {
  // The regression this file exists for: `workstream_committed` and five others
  // were emitted, rendered by the CLI, and silently dropped here.
  for (const [tag, payload] of Object.entries(ENGINE)) {
    const line = engineLine({ goal: "I1", payload }, 100);
    assert.ok(line, `${tag} produced no line`);
    assert.ok(line.text.length > 0, `${tag} rendered an empty line`);
    assert.equal(line.at, 100);
    assert.equal(line.goal, "I1");
  }
  assert.match(engineLine({ payload: ENGINE.git_setup_changed }, 1).text, /profiles/);
  assert.match(engineLine({ payload: ENGINE.connectors_changed }, 1).text, /connector accounts changed/);
  assert.match(engineLine({ payload: ENGINE.addons_changed }, 1).text, /clock/);
});

test("a workflow the Workflow Agent wrote says whose hand it was; a save by hand is a save", () => {
  assert.match(engineLine({ payload: ENGINE.workflow_changed }, 1).text, /saved as revision 2$/);
  const designed = engineLine({ payload: { ...ENGINE.workflow_changed, designed: true, revision: 3 } }, 1);
  assert.match(designed.text, /^the Workflow Agent wrote workflow .* as revision 3$/);
  assert.equal(designed.tone, "dim", "news for the Inbox, a line like any other on the Pulse");
});

test("a project's birth names where it was born", () => {
  assert.equal(
    engineLine({ payload: ENGINE.project_created }, 1).text,
    "project checkout created by a workflow step",
  );
  assert.equal(
    engineLine({ payload: { ...ENGINE.project_created, origin: { origin: "workspace" } } }, 1).text,
    "project checkout created",
  );
});

test("a repository nobody can commit in is a warning that says why, and its answer is a quiet line", () => {
  const asked = engineLine({ payload: ENGINE.committer_needed }, 1);
  assert.equal(asked.tone, "warn");
  assert.equal(asked.text, "project checkout needs someone to commit — an agent's work is kept uncommitted");
  assert.match(engineLine({ payload: { ...ENGINE.committer_needed, reason: "created" } }, 1).text, /just created$/);
  assert.match(engineLine({ payload: { ...ENGINE.committer_needed, reason: "commit_refused" } }, 1).text, /a commit was refused$/);
  const set = engineLine({ payload: ENGINE.committer_set }, 1);
  assert.equal(set.tone, "dim");
  assert.equal(set.text, "commits by Ada <ada@example.invalid>");
});

test("the workstream three say what actually changed", () => {
  assert.equal(
    engineLine({ payload: ENGINE.workstream_committed }, 1).text,
    "committed abcdef012345 on fix/total",
  );
  assert.equal(engineLine({ payload: ENGINE.workstream_opened }, 1).text, "workstream opened on fix/total");
  assert.equal(engineLine({ payload: ENGINE.workstream_changed }, 1).text, "workstream is dirty tree");
});

test("the guard's lines name the tool, who decided and why — and a refusal is a failure", () => {
  const denied = engineLine({ payload: ENGINE.guard_decided }, 1);
  assert.equal(denied.tone, "fail");
  assert.equal(denied.text, "refused Bash (rule privilege_escalation) — sudo make install: refused by the guard rule “sudo, doas, su”");
  const asked = engineLine({ payload: { ...ENGINE.guard_decided, verdict: "asked", by: "classifier", reason: "it phones home" } }, 1);
  assert.equal(asked.tone, "wait");
  assert.match(asked.text, /^Bash needs your answer \(the classifier\)/);
  const allowed = engineLine({ payload: { ...ENGINE.guard_decided, verdict: "allowed", by: "person", reason: null } }, 1);
  assert.equal(allowed.tone, "dim");
  assert.equal(allowed.text, "allowed Bash (you) — sudo make install");
  // The journal fact renders with the same words.
  const fact = payloadLine(JOURNAL.guard);
  assert.equal(fact.tone, "wait");
  assert.match(fact.text, /^Bash needs your answer \(the classifier\)/);
  assert.ok(fact.text.includes("«secret:bearer:0a1b2c»"), "the subject is shown as the journal has it — redacted");
  const redacted = engineLine({ payload: ENGINE.redacted }, 1);
  assert.equal(redacted.tone, "dim");
  assert.equal(redacted.text, "redacted 2 secrets before the prompt (github_token, env:MY_KEY)");
  assert.equal(engineLine({ payload: { ...ENGINE.redacted, count: 1, kinds: [], at: "mcp_reply" } }, 1).text, "redacted 1 secret before a tool's answer");
});

test("the Decision-Making Agent's lines name the point, what it chose and how the outcome stands", () => {
  const applied = engineLine({ payload: ENGINE.judged }, 1);
  assert.equal(applied.tone, "spine");
  assert.equal(applied.text, "Decision-Making Agent · assign.pick → developer (0.91) · applied");
  const unsure = engineLine({
    payload: { ...ENGINE.judged, judgement: { ...ENGINE.judged.judgement, outcome: "unsure", reason: "sure to 0.42, and this point acts from 0.70" } },
  }, 1);
  assert.equal(unsure.tone, "wait");
  assert.equal(unsure.text, "Decision-Making Agent · assign.pick · unsure — sure to 0.42, and this point acts from 0.70");
  const failed = engineLine({
    payload: { ...ENGINE.judged, judgement: { ...ENGINE.judged.judgement, outcome: "failed", answers: {}, reason: "the provider is misconfigured" } },
  }, 1);
  assert.equal(failed.tone, "fail");
  assert.equal(failed.text, "Decision-Making Agent · assign.pick · failed — the provider is misconfigured");
  // The journal fact renders with the same words.
  const fact = payloadLine(JOURNAL.judgement);
  assert.equal(fact.tone, "spine");
  assert.equal(fact.text, "Decision-Making Agent · assign.pick → developer (0.91) · applied");
  assert.ok(fact.detail.some((d) => d.label === "Provider" && d.value === "jev"));
});

test("a workstream script's line carries its reason, and a failure the danger tone", () => {
  const failed = engineLine({ payload: ENGINE.workstream_script_ran }, 1);
  assert.equal(failed.text, "post-create script exited with status 2", "the first line is the reason; the tail stays in the journal");
  assert.equal(failed.tone, "danger");
  const ok = engineLine({ payload: { ...ENGINE.workstream_script_ran, ok: true, output: "succeeded" } }, 1);
  assert.equal(ok.tone, "dim");
});

test("what a person approved and did not go out says the act and the first line of why, in the danger tone", () => {
  const line = engineLine({ payload: ENGINE.workstream_publish_failed }, 1);
  assert.equal(line.text, "could not push fix/total — no usable remote: the remote hung up");
  assert.equal(line.tone, "danger");
  assert.equal(line.icon, "mark:git", "a push that did not go out is git's, and wears the Git mark");
});

test("a failure is a failure and a wait is a wait", () => {
  assert.equal(engineLine({ payload: ENGINE.listener_failed }, 1).tone, "fail");
  assert.equal(
    engineLine({ payload: { ...ENGINE.step_changed, state: "failed" } }, 1).tone,
    "fail",
  );
  assert.equal(engineLine({ payload: ENGINE.step_changed }, 1).tone, "dim", "a step starting is ambient");
  assert.equal(engineLine({ payload: { ...ENGINE.run_finished, outcome: "failed" } }, 1).tone, "fail");
  assert.equal(engineLine({ payload: ENGINE.run_cancelled }, 1).tone, "dim", "a stop is the person's own act, never a failure");
  assert.equal(engineLine({ payload: ENGINE.run_cancelled }, 1).text, "run 00RUN1 stopped");
  assert.equal(engineLine({ payload: { ...ENGINE.run_cancelled, cause: { cause: "withdrawn" } } }, 1).text, "run 00RUN1 withdrawn");
  assert.equal(engineLine({ payload: ENGINE.run_queued }, 1).tone, "dim");
  assert.equal(engineLine({ payload: ENGINE.run_queued }, 1).text, "run 00RUN1 queued (2nd in line) on workflow 000WF1");
  assert.equal(engineLine({ payload: { ...ENGINE.run_queued, position: 1 } }, 1).text, "run 00RUN1 queued (next in line) on workflow 000WF1");
  assert.equal(engineLine({ payload: ENGINE.workflow_proposed }, 1).tone, "wait");
  assert.equal(engineLine({ payload: ENGINE.gate_opened }, 1).tone, "wait");
  assert.equal(engineLine({ payload: ENGINE.question_asked }, 1).tone, "wait");
  assert.equal(engineLine({ payload: ENGINE.paused }, 1).tone, "wait");
  // A completed execution recedes; anything else rises.
  assert.equal(engineLine({ payload: ENGINE.execution_ended }, 1).tone, "dim");
  assert.equal(
    engineLine({ payload: { type: "execution_ended", outcome: { outcome: "aborted" } } }, 1).tone,
    "fail",
  );
  assert.match(
    engineLine({ payload: { type: "execution_ended", outcome: { outcome: "failed", reason: "no tests" } } }, 1).text,
    /failed: no tests/,
  );
  // Presence: waiting is owed to a human; the streaming states are silent.
  assert.equal(engineLine({ payload: ENGINE.session_state }, 1).tone, "wait");
  assert.match(engineLine({ payload: ENGINE.session_state }, 1).text, /permission: Bash/);
  const running = { ...ENGINE.session_state, presence: { ...ENGINE.session_state.presence, state: { state: "running", tool: "Read", args: "" } } };
  assert.equal(engineLine({ payload: running }, 1), null, "a tool starting is the roster's, not the activity's");
});

test("an engine payload this build has never heard of is skipped, not crashed on", () => {
  assert.equal(engineLine({ payload: { type: "invented_later" } }, 1), null);
});

// ---------------------------------------------------------------------------
// Session events
// ---------------------------------------------------------------------------

test("session lifecycle renders, and turn boundaries deliberately do not", () => {
  const line = (event) => sessionLine({ tier: "lifecycle", event });
  assert.equal(line({ type: "started" }).text, "session started");
  assert.equal(line({ type: "parked" }).text, "session parked");
  assert.equal(line({ type: "revived" }).text, "session revived");
  assert.equal(
    line({ type: "input_requested", request: { id: "r", kind: "permission", tool_name: "Bash", tier: "exec", args_summary: "" } }).text,
    "asked to run Bash",
  );
  assert.match(line({ type: "input_requested", request: { id: "r", kind: "question", text: "Which tone?" } }).text, /Which tone\?/);
  assert.equal(line({ type: "input_resolved", id: "r" }).tone, "dim");
  const progress = (event) => sessionLine({ tier: "progress", event });
  assert.match(progress({ type: "subagent_started", id: "t", name: "explore", description: "look" }).text, /sub-agent explore/);
  assert.match(
    progress({ type: "nested", parent: "t", event: { type: "tool_started", name: "Read", args_summary: "x", tier: "read" } }).text,
    /^↳ Read/,
    "a sub-agent's tool is prefixed, never the agent's own",
  );
  assert.equal(progress({ type: "nested", parent: "t", event: { type: "text_delta", text: "hi" } }), null);
  // Not terminal: a turn ended, not the session. Rendering it would put a
  // "session completed" line in the activity for every turn.
  assert.equal(line({ type: "ended", is_terminal: false, outcome: { outcome: "completed" } }), null);
  assert.equal(
    line({ type: "ended", is_terminal: true, outcome: { outcome: "completed" } }).text,
    "session completed",
  );
  assert.equal(
    line({ type: "ended", is_terminal: true, outcome: { outcome: "failed", error: "boom" } }).tone,
    "fail",
  );
  // A model that went away is a retry on the next model, so it waits rather
  // than fails — and it says which model, which "session suspended" did not.
  const gone = line({
    type: "ended",
    is_terminal: true,
    outcome: { outcome: "model_unavailable", model: "big", reason: "rate limited" },
  });
  assert.equal(gone.tone, "wait");
  assert.match(gone.text, /big/);
});

test("raw output and streamed text stay out of a stream of actions", () => {
  assert.equal(sessionLine({ tier: "raw", event: {} }), null);
  assert.equal(sessionLine({ tier: "progress", event: { type: "turn_started" } }), null);
  assert.equal(sessionLine({ tier: "progress", event: { type: "text_delta", text: "hi" } }), null);
  // A tool that succeeded is noise; one that failed is not.
  assert.equal(sessionLine({ tier: "progress", event: { type: "tool_ended", name: "Bash", ok: true } }), null);
  assert.equal(
    sessionLine({ tier: "progress", event: { type: "tool_ended", name: "Bash", ok: false } }).tone,
    "fail",
  );
});

// ---------------------------------------------------------------------------
// Conversation frames
// ---------------------------------------------------------------------------

test("a live message row points at the conversation it came from", () => {
  // Without this the Pulse rendered every live message as a disabled button:
  // the view disables a row with no scope, and the scope was never set.
  const line = conversationLine({ scope: "C1", kind: 9, event_id: "e1", author: "abc", snippet: "hi" }, 7);
  assert.equal(line.goal, "C1");
  assert.equal(line.author, "abc");
  assert.equal(line.text, "hi");
  assert.equal(line.tone, "dim");
  assert.equal(line.icon, undefined);
});

test("a snapshot frame is the backlog arriving, not something happening", () => {
  assert.equal(conversationLine({ scope: "C1", kind: 9, snapshot: true }, 1), null);
});

test("a message with no snippet still says something happened", () => {
  assert.equal(conversationLine({ scope: "C1", kind: 9, event_id: "e1" }, 1).text, "posted a message");
});

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

test("truncate flattens whitespace before it counts characters", () => {
  assert.equal(truncate("a\n  b   c"), "a b c");
  assert.equal(truncate("abcdef", 4), "abc…");
  assert.equal(truncate("abcd", 4), "abcd");
});

test("shortId takes the tail, which is the part that differs", () => {
  assert.equal(shortId("01J000000000000000WORK1"), "0WORK1");
  assert.equal(shortId(null), "");
  assert.equal(shortId(undefined), "");
});

test("cents becomes dollars once there are dollars to show", () => {
  assert.equal(cents(7), "7¢");
  assert.equal(cents(99), "99¢");
  assert.equal(cents(100), "$1.00");
  assert.equal(cents(1234), "$12.34");
});

test("appendCapped drops from the old end, never the new one", () => {
  const line = (n) => ({ key: `k${n}`, at: n, text: `${n}`, tone: "dim" });
  let lines = [];
  for (let i = 0; i < 5; i++) lines = appendCapped(lines, line(i), 3);
  assert.deepEqual(
    lines.map((l) => l.text),
    ["2", "3", "4"],
  );
});

test("the Workflow Agent's standing reads as news, trouble or work, by status", () => {
  assert.equal(guidedWords("design", "working", "on claude-code").tone, "spine");
  assert.match(guidedWords("design", "working", null).text, /designing the workflow/);
  assert.match(guidedWords("repair", "working", null).text, /repairing the workflow/);
  assert.equal(guidedWords("design", "asking", "Which tone?").tone, "wait");
  assert.equal(guidedWords("design", "proposed", "Ship it (3 steps)").tone, "wait");
  assert.match(guidedWords("design", "proposed", "Ship it (3 steps)").text, /adopt it\?$/);
  assert.equal(guidedWords("design", "stalled", "no proposal after 600s").tone, "fail");
  assert.match(guidedWords("design", "stalled", "no proposal after 600s").text, /no proposal after 600s/);
  assert.equal(guidedWords("design", "failed", "cannot launch").tone, "fail");
  assert.equal(guidedWords("design", "off", null).tone, "dim");
  assert.equal(guidedWords("design", "scheduled", null).tone, "dim");
});

test("a document given to the goal names the file, and its detail carries type, size and hash", () => {
  const line = payloadLine(JOURNAL.document);
  assert.equal(line.text, "gave the document brief.pdf");
  assert.equal(line.tone, "spine");
  assert.equal(line.icon, "icon:document");
  assert.deepEqual(line.detail.find((f) => f.label === "Type"), { label: "Type", value: "application/pdf" });
  assert.equal(line.detail.find((f) => f.label === "Size").value, "1.2 KB");
  assert.equal(line.detail.find((f) => f.label === "Hash").value, "ab".repeat(32));
  const live = engineLine({ goal: "G1", payload: ENGINE.document_added }, 1);
  assert.equal(live.text, line.text, "the live frame and the stored fact read the same");
  assert.equal(live.tone, "spine");
});

test("a failed run's payloads with their fields missing still make a line — one stored row never takes the feed", () => {
  const at = 1_700_000_000;
  const line = (payload) => engineLine({ payload }, at);
  assert.equal(line({ type: "execution_ended" }).tone, "fail", "no outcome reads as not completed");
  assert.match(line({ type: "execution_ended" }).text, /execution ended/);
  assert.equal(line({ type: "session_state" }), null, "no presence is no line, not a throw");
  assert.equal(line({ type: "session_state", presence: { harness: "h" } }), null);
  assert.match(line({ type: "workstream_changed" }).text, /workstream is changed/);
  assert.match(line({ type: "document_added" }).text, /gave the document/);
  assert.equal(sessionLine(null), null);
  assert.equal(sessionLine({ tier: "lifecycle" }), null, "no event is no line");
  assert.match(sessionLine({ tier: "lifecycle", event: { type: "ended", is_terminal: true } }).text, /session suspended/);
});

test("a stored row whose event this build cannot word is its type as a dim line", () => {
  const row = { seq: 9, at: 1_700_000_000, concept: "goals", source: { kind: "goal", id: "g" }, event: { type: "something_new", payload: { deep: { deeper: 1 } } } };
  const line = pulseLine(row);
  assert.equal(line.tone, "dim");
  assert.equal(line.text, "something new");
  assert.equal(line.key, "p:9");
  const bare = pulseLine({ seq: 10, at: 1_700_000_000, concept: "goals", source: { kind: "goal", id: "g" }, event: "not an object" });
  assert.equal(bare.tone, "dim");
  assert.equal(bare.text, "unknown");
});

test("the collaboration payloads read as people, invitations, held messages and relays", () => {
  const who = "ab".repeat(32);
  assert.match(engineLine({ payload: { type: "people_changed", pubkey: who, change: { change: "joined", role: "guest" }, label: "Bob" } }, 1).text, /Bob joined as a guest/);
  assert.match(engineLine({ payload: { type: "people_changed", pubkey: who, change: { change: "left" } } }, 1).text, /left/);
  assert.match(engineLine({ payload: { type: "people_changed", pubkey: who, change: { change: "role_changed", role: "member" } } }, 1).text, /now a member/);
  const asked = engineLine({ payload: { type: "invite_changed", invite: { id: "i", role: "guest", channels: [], created_at: 1, expires_at: 2, state: { state: "requested", by: who, label: "Bob", at: 1 } } } }, 1);
  assert.match(asked.text, /Bob asked to join/);
  assert.equal(asked.tone, "wait");
  assert.match(engineLine({ payload: { type: "invite_changed", invite: { id: "i", role: "member", channels: [], created_at: 1, expires_at: 2, state: { state: "pending" } } } }, 1).text, /made \(member\)/);
  const held = engineLine({ payload: { type: "message_held", scope: "design", event: "e", author: who, reason: { reason: "harmful", why: "asks for a token" } } }, 1);
  assert.match(held.text, /held — asks for a token/);
  assert.equal(held.tone, "fail");
  assert.match(engineLine({ payload: { type: "message_held", scope: "design", event: "e", author: who, reason: { reason: "pending" } } }, 1).text, /being read/);
  assert.match(engineLine({ payload: { type: "message_released", scope: "design", event: "e" } }, 1).text, /let through/);
  const withheld = engineLine({ payload: { type: "content_screened", scope: "c1", agent: "general-agent", source: "evil.example", verdict: "withheld" } }, 1);
  assert.match(withheld.text, /content from evil.example was withheld from general-agent/);
  assert.equal(withheld.tone, "fail");
  assert.match(engineLine({ payload: { type: "content_screened", agent: "general-agent", source: "docs.example", verdict: "safe" } }, 1).text, /screened safe/);
  assert.match(engineLine({ payload: { type: "content_screened", agent: "general-agent", source: "docs.example", verdict: "allowed" } }, 1).text, /allowed by you/);
  assert.equal(engineLine({ payload: { type: "content_screened", agent: "general-agent", source: "docs.example", verdict: "unscreened" } }, 1).tone, "dim");
  assert.match(engineLine({ payload: { type: "hosted_changed", host: who, scope: "general" } }, 1).text, /moved in general/);
  assert.match(engineLine({ payload: { type: "relays_changed" } }, 1).text, /relays changed/);
});

test("an event's line says what it did: a run on the goal that listens, a run of the workspace and no goal, or none, and why", () => {
  const onGoal = engineLine({ goal: "I1", payload: ENGINE.listener_fired }, 1);
  const inWorkspace = engineLine({ payload: { type: "listener_fired", listener: `workspace:${WORKFLOW}/ticket`, signal: "S1", outcome: { outcome: "started", run: RUN } } }, 1);
  assert.equal(onGoal.text, "an event started run 00RUN1 on goal I1");
  assert.equal(inWorkspace.text, "an event started run 00RUN1 in the workspace");
  assert.ok(!inWorkspace.text.includes("goal"), inWorkspace.text);
  assert.equal(inWorkspace.tone, "spine");
  assert.equal(inWorkspace.icon, "icon:signal", "the bolt is the event's");
  const skipped = engineLine({ payload: { type: "listener_fired", listener: `workspace:${WORKFLOW}/ticket`, signal: "S2", outcome: { outcome: "skipped", reason: "the last run is not finished" } } }, 1);
  assert.equal(skipped.text, "an event started no run: the last run is not finished");
  assert.equal(skipped.tone, "dim", "a guard doing its work is ambient, never a failure");
  assert.deepEqual(skipped.detail.find((f) => f.label === "Listener"), { label: "Listener", value: `workspace:${WORKFLOW}/ticket` });
  assert.equal(skipped.detail.find((f) => f.label === "Run"), undefined, "no run, no empty field");
});

test("a start event that could not start a run is a failure — a workflow's or a goal's, by its listener", () => {
  const wf = engineLine({ payload: ENGINE.listener_failed }, 1);
  assert.equal(wf.tone, "fail");
  assert.equal(wf.text, "a workflow's start event could not start a run: the inputs no longer bind: ticket");
  assert.deepEqual(wf.detail.find((f) => f.label === "Error"), { label: "Error", value: "the inputs no longer bind: ticket" });
  const goal = engineLine({ payload: { ...ENGINE.listener_failed, listener: "goal:01J0GOALCREATED/ticket", signal: undefined } }, 1);
  assert.match(goal.text, /^a goal's start event could not start a run: /);
  assert.equal(goal.detail.find((f) => f.label === "Signal"), undefined, "an arming that failed names no occurrence");
});

test("listening turned on or off says whose, and leaves the word it cannot name to the detail", () => {
  const on = engineLine({ payload: ENGINE.listening_changed }, 1);
  assert.equal(on.text, "a workflow started listening");
  assert.equal(on.tone, "dim");
  assert.deepEqual(on.detail, [{ label: "Host", value: `workspace:${WORKFLOW}` }]);
  assert.equal(engineLine({ payload: { ...ENGINE.listening_changed, on: false } }, 1).text, "a workflow stopped listening");
  assert.equal(engineLine({ payload: { type: "listening_changed", host: "goal:01J0GOALCREATED", on: true } }, 1).text, "a goal started listening");
  assert.equal(hostOf("goal:G1/ticket"), "goal");
  assert.equal(hostOf(`workspace:${WORKFLOW}`), "workflow");
  assert.equal(hostOf(null), "workflow", "a word of another shape reads as the workspace's, never a throw");
});

test("an event queued says where it came from and who heard it; one nobody heard is recorded for the waits", () => {
  const queued = engineLine({ payload: ENGINE.signal_received }, 1);
  assert.equal(queued.text, "hook event queued for a workflow");
  assert.equal(queued.tone, "dim");
  assert.equal(engineLine({ payload: { ...ENGINE.signal_received, listener: "goal:I1/ticket" } }, 1).text, "hook event queued for a goal");
  const kept = engineLine({ payload: { type: "signal_received", signal: "S3", listener: null, source: "signal" } }, 1);
  assert.equal(kept.text, "signal event recorded");
  assert.equal(kept.detail.find((f) => f.label === "Listener"), undefined, "no listener, no empty field");
});

test("a boundary that diverts is the run's story; one that acts beside its step is ambient", () => {
  const divert = engineLine({ payload: ENGINE.boundary_fired }, 1);
  assert.equal(divert.text, "step review diverted by boundary late");
  assert.equal(divert.tone, "spine");
  assert.ok(divert.detail.some((f) => f.label === "Boundary" && f.value === "late"));
  const act = engineLine({ payload: { ...ENGINE.boundary_fired, boundary: "nudge", diverts: false } }, 1);
  assert.equal(act.text, "boundary nudge acted beside step review");
  assert.equal(act.tone, "dim");
});

test("the event that started a run is on its home's record: a named signal by its name, any other event by its source", () => {
  const line = payloadLine(JOURNAL.signal);
  assert.equal(line.text, "hook event started this run");
  assert.equal(line.icon, "icon:signal");
  assert.deepEqual(line.detail.find((f) => f.label === "Listener"), { label: "Listener", value: `workspace:${WORKFLOW}/ticket` });
  assert.equal(line.detail.find((f) => f.label === "Name"), undefined, "an unnamed event has no name field");
  const named = payloadLine({ ...JOURNAL.signal, source: "signal", name: "report.ready" });
  assert.equal(named.text, "signal report.ready started this run");
  assert.deepEqual(named.detail.find((f) => f.label === "Name"), { label: "Name", value: "report.ready" });
});

test("a goal comes to exist three ways — captured, spawned from a goal, made by a run — each in its own words", () => {
  const made = (origin) => engineLine({ payload: { type: "goal_created", goal: "G00001", origin } }, 1).text;
  assert.equal(made({ origin: "captured" }), "goal G00001 captured");
  assert.equal(made({ origin: "spawned", parent: "G00000" }), "goal G00001 spawned");
  assert.equal(made({ origin: "run", run: RUN, step: "split" }), "goal G00001 spawned by run 00RUN1");
  assert.equal(made(undefined), "goal G00001 captured", "no origin reads as captured, never a throw");
});
