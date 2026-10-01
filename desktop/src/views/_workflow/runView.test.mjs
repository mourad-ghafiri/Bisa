/**
 * How a run reads, tested where it lives.
 *
 * The tones are theme roles, so the one thing worth checking against another
 * file is that every role named here is one the theme contract answers —
 * a tone the contract lacks is a ring that paints as nothing.
 *
 * Run with `npm test` from `desktop/`.
 */

import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import {
  LEGEND_STATES,
  STEP_TONE_TOKENS,
  branchesChosen,
  currentSteps,
  edgeTone,
  failedStep,
  firedBoundaries,
  overlayFacts,
  progress,
  stepActions,
  stepLabel,
  stepStateWord,
  stepTone,
} from "./runView.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));

function roles() {
  const css = readFileSync(join(HERE, "../../theme/tokens.css"), "utf8");
  const region = css.match(/@roles:start\s*\*\/([\s\S]*?)\/\*\s*@roles:end/);
  return new Set([...region[1].matchAll(/--color-([a-z0-9-]+)\s*:/g)].map((m) => m[1]));
}

function states() {
  const src = readFileSync(join(HERE, "../../../../crates/bisa-core/src/run.rs"), "utf8");
  const m = src.match(/pub enum StepState \{([\s\S]*?)\n\}/);
  return [...m[1].matchAll(/^\s{4}([A-Z][A-Za-z]*)/gm)].map((x) => x[1].toLowerCase());
}

const run = {
  workflow: {
    steps: [
      { id: "a", name: "A", kind: "agent", instructions: "", then: [{ to: "v" }] },
      { id: "v", name: "V", kind: "decide", rules: [], otherwise: "no", then: [{ to: "b", branch: "yes" }, { to: "c", branch: "no" }] },
      { id: "b", name: "B", kind: "human", prompt: "?", then: [] },
      { id: "c", name: "C", kind: "wait", until: { until: "release" }, then: [] },
    ],
  },
  steps: {
    a: { state: { state: "done" }, work_item: "01J0000000000000000ITEM1" },
    v: { state: { state: "done", branches: ["yes"] } },
    b: { state: { state: "waiting" } },
    c: { state: { state: "skipped" } },
  },
};

test("every step state has a tone, and every tone is a theme role", () => {
  assert.deepEqual(Object.keys(STEP_TONE_TOKENS).sort(), states().sort());
  const known = roles();
  for (const role of Object.values(STEP_TONE_TOKENS)) assert.ok(known.has(role), `${role} is a role`);
});

test("a step's tone and label follow its record", () => {
  assert.equal(stepTone(run, "b"), "warn");
  assert.equal(stepLabel(run, "b"), "waiting on you");
  assert.equal(stepLabel(run, "v"), "done → yes");
  assert.equal(stepLabel(run, "a"), "done");
  assert.equal(stepLabel(run, "nope"), "pending");
  assert.equal(stepTone(null, "a"), "text-dim");
  const failed = { steps: { x: { state: { state: "failed" }, error: "boom" } } };
  assert.equal(stepLabel(failed, "x"), "failed: boom");
  const running = { steps: { x: { state: { state: "running" }, work_item: "01J0000000000000000ITEM1" } } };
  assert.equal(stepLabel(running, "x"), "running (item 0ITEM1)");
  // A loop mid-way names its place; a finished loop reads like any done step.
  const looping = { steps: { f: { state: { state: "done", branches: ["each"] }, cursor: { items: [1, 2, 3, 4, 5], index: 2 } } } };
  assert.equal(stepLabel(looping, "f"), "each 2 of 5");
  const whiling = { steps: { w: { state: { state: "done", branches: ["loop"] }, cursor: { index: 3 } } } };
  assert.equal(stepLabel(whiling, "w"), "loop 3");
  const finished = { steps: { f: { state: { state: "done", branches: ["done"] } } } };
  assert.equal(stepLabel(finished, "f"), "done → done");
  // Every rule that held names its branch; a divert names the boundary that stopped the step.
  const every = { steps: { d: { state: { state: "done", branches: ["urgent", "billing"] } } } };
  assert.equal(stepLabel(every, "d"), "done → urgent, billing");
  const diverted = { steps: { r: { state: { state: "diverted", by: "late" } } } };
  assert.equal(stepLabel(diverted, "r"), "diverted → late");
  assert.equal(stepTone(diverted, "r"), "warn", "a detour, not a failure");
  assert.deepEqual(branchesChosen({ state: "done" }), [], "a task chose nothing");
  assert.deepEqual(branchesChosen({ state: "diverted", by: "late" }), []);
});

test("progress counts what reached an end, skipped included", () => {
  assert.equal(progress(run), 0.75);
  assert.equal(progress(null), 0);
  assert.equal(progress({ workflow: { steps: [] } }), 0);
});

test("a start and a parallel are no work of their own; a diverted step reached its end", () => {
  const routed = {
    workflow: {
      steps: [
        { id: "begin", kind: "start", on: { event: "manual" } },
        { id: "fan", kind: "parallel" },
        { id: "review", kind: "approval", prompt: "?" },
        { id: "escalate", kind: "agent", instructions: "" },
      ],
    },
    steps: {
      begin: { state: { state: "done" } },
      fan: { state: { state: "done" } },
      review: { state: { state: "diverted", by: "late" } },
      escalate: { state: { state: "running" } },
    },
  };
  assert.equal(progress(routed), 0.5, "one of the two steps of work reached its end");
  assert.equal(progress({ workflow: { steps: [{ id: "begin", kind: "start" }] }, steps: {} }), 0, "a workflow of nothing but a start has no work to count");
});

test("the boundary events that fired this visit light their chips", () => {
  const r = { steps: { review: { state: { state: "waiting" }, fired: { nudge: { count: 2, seq: 9, at: 100 }, late: { count: 0, seq: 0, at: 0 } } } } };
  assert.deepEqual([...firedBoundaries(r, "review")], ["nudge"]);
  assert.equal(firedBoundaries(r, "nope").size, 0);
  assert.equal(firedBoundaries(null, "review").size, 0);
});

test("an unlabelled flow is taken only when the step chose no branch; every rule that held takes its flow", () => {
  const every = { steps: { d: { state: { state: "done", branches: ["urgent", "billing"] } } } };
  assert.equal(edgeTone(every, { from: "d", to: "a", branch: "urgent", kind: "then" }), "taken");
  assert.equal(edgeTone(every, { from: "d", to: "b", branch: "billing", kind: "then" }), "taken");
  assert.equal(edgeTone(every, { from: "d", to: "c", branch: "otherwise", kind: "then" }), "skipped");
  assert.equal(edgeTone(every, { from: "d", to: "x", branch: null, kind: "then" }), "skipped", "a gateway that chose leaves its unlabelled flows");
  const task = { steps: { t: { state: { state: "done" } } } };
  assert.equal(edgeTone(task, { from: "t", to: "x", branch: null, kind: "then" }), "taken");
  assert.equal(edgeTone(task, { from: "t", to: "late", branch: "late", kind: "boundary" }), "skipped", "a step that finished never took a divert's path");
});

test("a diverted step takes only the boundary's path — never its own flow, never its on-fail route", () => {
  const diverted = { steps: { r: { state: { state: "diverted", by: "late" } } } };
  assert.equal(edgeTone(diverted, { from: "r", to: "escalate", branch: "late", kind: "boundary" }), "taken");
  assert.equal(edgeTone(diverted, { from: "r", to: "cancel", branch: "cancelled", kind: "boundary" }), "skipped", "another boundary did not fire");
  assert.equal(edgeTone(diverted, { from: "r", to: "ship", branch: null, kind: "then" }), "skipped");
  assert.equal(edgeTone(diverted, { from: "r", to: "fix", branch: null, kind: "on_fail" }), "skipped");
  const live = { steps: { r: { state: { state: "waiting" } } } };
  assert.equal(edgeTone(live, { from: "r", to: "escalate", branch: "late", kind: "boundary" }), "default", "still open while the step is live");
});

test("the live steps are the running and waiting ones", () => {
  assert.deepEqual(currentSteps(run), ["b"]);
});

test("a person's actions on a step come from its kind and state", () => {
  assert.deepEqual(stepActions(run.workflow.steps[2], run.steps.b), ["answer", "done"]);
  assert.deepEqual(stepActions(run.workflow.steps[3], { state: { state: "waiting" } }), ["release"]);
  assert.deepEqual(stepActions({ kind: "approval" }, { state: { state: "waiting" } }), ["decide"]);
  assert.deepEqual(stepActions({ kind: "wait", until: { until: "delay", secs: 5 } }, { state: { state: "waiting" } }), []);
  assert.deepEqual(stepActions(run.workflow.steps[0], run.steps.a), ["open"]);
  assert.deepEqual(stepActions(run.workflow.steps[2], { state: { state: "done" } }), []);
});

test("an edge is taken by the branch chosen and skipped by the one that was not", () => {
  const yes = { from: "v", to: "b", branch: "yes", kind: "then" };
  const no = { from: "v", to: "c", branch: "no", kind: "then" };
  const plain = { from: "a", to: "v", branch: null, kind: "then" };
  const open = { from: "b", to: "z", branch: null, kind: "then" };
  assert.equal(edgeTone(run, yes), "taken");
  assert.equal(edgeTone(run, no), "skipped");
  assert.equal(edgeTone(run, plain), "taken");
  assert.equal(edgeTone(run, open), "default");
  const fail = { from: "x", to: "y", branch: null, kind: "on_fail" };
  assert.equal(edgeTone({ steps: { x: { state: { state: "failed" } } } }, fail), "taken");
  assert.equal(edgeTone({ steps: { x: { state: { state: "done" } } } }, fail), "skipped");
  assert.equal(edgeTone({ steps: {} }, fail), "default");
});

test("a failed run names the step it failed at — the newest failed record — with its error; a run that did not fail names nothing", () => {
  const run = {
    outcome: "failed",
    workflow: { steps: [{ id: "build", name: "Build it" }, { id: "test", name: "Test it" }] },
    steps: {
      build: { state: { state: "failed" }, seq: 2, error: "first failure" },
      test: { state: { state: "failed" }, seq: 5, error: "interrupted by a restart" },
    },
  };
  assert.deepEqual(failedStep(run), { id: "test", name: "Test it", error: "interrupted by a restart" });
  assert.equal(failedStep({ ...run, outcome: "done" }), null);
  assert.equal(failedStep({ outcome: "failed", steps: {} }), null, "no failed step is no sentence");
  assert.equal(failedStep(null), null);
  assert.deepEqual(failedStep({ outcome: "failed", steps: { x: { state: { state: "failed" } } } }), { id: "x", name: "x", error: null }, "a step the workflow no longer names keeps its id");
});

/** The core's edge rule, read from the source: what a flow out of a failed step is. */
function coreEdgeRule() {
  const src = readFileSync(join(HERE, "../../../../crates/bisa-core/src/run.rs"), "utf8");
  const body = src.match(/fn edge\(&self, from: &Step, flow: &Flow\) -> Edge \{([\s\S]*?)\n {4}\}/);
  assert.ok(body, "WorkflowRun::edge in run.rs");
  return body[1].replace(/\s+/g, " ");
}

test("a failure passed over takes the step's own flow, one routed takes its route — the core's edge rule", () => {
  const rule = coreEdgeRule();
  assert.ok(rule.includes("OnFail::Skip if flow.branch.is_none() => Edge::Taken"), "the core: a skip takes the unlabelled flows");
  assert.ok(rule.includes("OnFail::Then { step } if &flow.to == step => Edge::Taken"), "the core: a route takes the flow to its step");
  const failed = (on_fail, then = [{ to: "d" }]) => ({
    outcome: null,
    workflow: { steps: [{ id: "c", name: "C", kind: "check", on_fail, then }, { id: "d", name: "D", kind: "agent" }, { id: "fix", name: "Fix", kind: "agent" }] },
    steps: { c: { state: { state: "failed" }, error: "exit 1", seq: 4 } },
  });
  const own = { from: "c", to: "d", branch: null, kind: "then" };
  assert.equal(edgeTone(failed({ on_fail: "skip" }), own), "taken", "the run went on along it: the path is lit");
  assert.equal(edgeTone(failed({ on_fail: "fail" }), own), "skipped", "a failure that ends the run takes nothing");
  assert.equal(edgeTone(failed(undefined), own), "skipped", "fail is the default");
  assert.equal(edgeTone(failed({ on_fail: "then", step: "fix" }), own), "skipped", "a routed failure leaves by its route alone");
  assert.equal(edgeTone(failed({ on_fail: "then", step: "fix" }), { from: "c", to: "fix", branch: null, kind: "on_fail" }), "taken");
  assert.equal(edgeTone(failed({ on_fail: "then", step: "d" }), own), "taken", "a flow to the very step the failure is routed to");
  assert.equal(edgeTone(failed({ on_fail: "skip" }), { from: "c", to: "d", branch: "yes", kind: "then" }), "skipped", "a failed step chose no branch");
  assert.equal(edgeTone(failed({ on_fail: "skip" }), { from: "c", to: "late", branch: "late", kind: "boundary" }), "skipped", "nor was it diverted");
  assert.equal(edgeTone({ steps: { c: { state: { state: "failed" } } } }, own), "skipped", "a run that names no workflow passes nothing over");
});

test("a spent step reads as a failed step with its error", () => {
  // `spent` is on the wire beside `error` (run.rs — `StepRecord.spent`): the step's visits ran out.
  const core = readFileSync(join(HERE, "../../../../crates/bisa-core/src/run.rs"), "utf8");
  assert.match(core, /pub spent: bool,/, "the record's field");
  assert.ok(core.includes("entered {} times; max_visits is {}") || core.includes("times; max_visits is"), "the error a spent step carries");
  const ring = {
    outcome: "done",
    started_at: 1,
    finished_at: 9,
    workflow: {
      steps: [
        { id: "a", name: "Draft", kind: "agent", instructions: "", on_fail: { on_fail: "skip" }, max_visits: 3, then: [{ to: "b" }] },
        { id: "b", name: "Review", kind: "human", prompt: "?", on_fail: { on_fail: "skip" }, max_visits: 3, then: [{ to: "a" }] },
      ],
    },
    steps: {
      a: { state: { state: "failed" }, spent: true, visits: 3, seq: 11, error: "entered 3 times; max_visits is 3", work_item: "01J0000000000000000ITEM1" },
      b: { state: { state: "failed" }, spent: true, visits: 3, seq: 12, error: "entered 3 times; max_visits is 3" },
    },
  };
  assert.equal(stepTone(ring, "a"), "danger");
  assert.equal(stepLabel(ring, "a"), "failed: entered 3 times; max_visits is 3");
  assert.equal(progress(ring), 1, "a failed step reached its end");
  assert.deepEqual(currentSteps(ring), [], "nothing is live on a spent step");
  assert.deepEqual(stepActions(ring.workflow.steps[1], ring.steps.b), [], "a spent human step asks nothing");
  assert.deepEqual(stepActions(ring.workflow.steps[0], ring.steps.a), ["open"], "its work item is still there to read");
  assert.equal(failedStep(ring), null, "the ring passed over its failures: the run is done, and names no step it failed at");
  assert.deepEqual(failedStep({ ...ring, outcome: "failed" }), { id: "b", name: "Review", error: "entered 3 times; max_visits is 3" });
  // The flows out of a spent step were taken once, when it failed and was passed over.
  assert.equal(edgeTone(ring, { from: "a", to: "b", branch: null, kind: "then" }), "taken");
  // The same record without the field reads the same: `spent` moves no word.
  const plain = { ...ring, steps: { ...ring.steps, a: { ...ring.steps.a, spent: undefined } } };
  assert.equal(stepLabel(plain, "a"), stepLabel(ring, "a"));
});

test("the strip above a run's canvas: the run's own status, its place among the runs, how far it got and when", () => {
  const wf = { revision: 4, steps: [{ id: "a", kind: "agent" }, { id: "b", kind: "human" }] };
  const waiting = { id: "r2", workflow: wf, queued_at: 5, started_at: 10, outcome: null, cancelled: null, steps: { a: { state: { state: "done" } }, b: { state: { state: "waiting" } } } };
  const runs = [
    { id: "r2", status: "waiting", queued_at: 5, started_at: 10 },
    { id: "r1", status: "done", queued_at: 1, started_at: 1, finished_at: 4 },
  ];
  const facts = overlayFacts(waiting, runs);
  assert.deepEqual(
    { status: facts.status, word: facts.word, tone: facts.tone, queued: facts.queued, index: facts.index, revision: facts.revision, percent: facts.percent, live: facts.live },
    { status: "waiting", word: "waiting", tone: "warn", queued: false, index: 2, revision: 4, percent: 50, live: ["b"] },
    "a run whose one live step waits on a person waits — the header's word, the Runs pane's and the strip's are one",
  );
  assert.deepEqual(facts.began, { word: "started", at: 10 });
  assert.equal(facts.ended, null);
  const runningNow = overlayFacts({ ...waiting, steps: { a: { state: { state: "running" } }, b: { state: { state: "waiting" } } } }, runs);
  assert.deepEqual([runningNow.status, runningNow.tone], ["running", "accent"]);

  // Queued behind the live run: its place from the list, no progress to draw.
  const behind = overlayFacts({ id: "r3", workflow: wf, queued_at: 20, started_at: null, outcome: null, cancelled: null, steps: {} }, [{ id: "r3", status: "queued", queued_at: 20, position: 2 }, ...runs]);
  assert.deepEqual([behind.status, behind.word, behind.queued, behind.index], ["queued", "queued · 2nd in line", true, 3]);
  assert.deepEqual(behind.began, { word: "queued", at: 20 });

  // How it ended: the outcome, or the cause of a cancel — never a status the list no longer says.
  const stopped = overlayFacts({ ...waiting, finished_at: 30, cancelled: { cause: "stopped", rationale: null } }, runs);
  assert.deepEqual([stopped.status, stopped.word, stopped.tone], ["cancelled", "stopped", "quiet"]);
  assert.deepEqual(stopped.ended, { word: "stopped", at: 30 });
  const failed = overlayFacts({ ...waiting, finished_at: 31, outcome: "failed", steps: { a: { state: { state: "failed" }, spent: true, error: "entered 3 times; max_visits is 3" } } }, []);
  assert.deepEqual([failed.status, failed.tone, failed.index, failed.percent], ["failed", "danger", null, 50]);
  assert.deepEqual(failed.ended, { word: "finished", at: 31 });
});

test("every state the core names has a word, and the legend names states it has", () => {
  const core = states();
  for (const state of core) assert.notEqual(stepStateWord(state), "", state);
  assert.deepEqual(core.map(stepStateWord).sort(), ["cancelled", "diverted", "done", "failed", "pending", "running", "skipped", "waiting"]);
  for (const state of LEGEND_STATES) assert.ok(core.includes(state), `${state} is a state`);
  assert.equal(stepStateWord("molten"), "molten", "a state this build has no word for is said as the node said it");
  assert.equal(stepStateWord("toString"), "toString", "and never a word that only looks like one");
  // No word of a label is written in the model: each is the catalog's.
  const model = readFileSync(join(HERE, "runView.mjs"), "utf8");
  const label = model.slice(model.indexOf("export function stepLabel("), model.indexOf("export function progress("));
  assert.ok(label.includes("switch (state)"), "stepLabel in runView.mjs");
  const said = label.split("\n").filter((line) => !/^\s*case "/.test(line)).join("\n");
  assert.deepEqual(said.match(/[`"](running|done|failed|skipped|cancelled|pending|diverted)\b[^"`\n]*[`"]\s*[;:]/g) ?? [], [], "a label's word is a message");
});
