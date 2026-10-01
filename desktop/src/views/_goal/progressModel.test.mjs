import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { finishedWords, liveSteps, progressCount, progressRows, rowOpen, rowPressed, stepHolder } from "./progressModel.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const CORE = join(HERE, "../../../../crates/bisa-core/src");

function variantsOf(file, name) {
  const src = readFileSync(join(CORE, file), "utf8");
  const body = src.match(new RegExp(`pub enum ${name} \\{([\\s\\S]*?)\\n\\}`));
  assert.ok(body, `${name} in ${file}`);
  return [...body[1].matchAll(/^\s{4}([A-Z][A-Za-z]*)[ ,{(]/gm)].map((m) => m[1].replace(/([a-z])([A-Z])/g, "$1_$2").toLowerCase());
}

test("every core step kind has a holder when waiting, and the ladder matches waiting_holder", () => {
  for (const kind of variantsOf("workflow.rs", "StepKind")) {
    assert.ok(stepHolder(kind, "waiting") !== null, `${kind} waiting has a holder`);
    assert.equal(stepHolder(kind, "running"), "agents");
    assert.equal(stepHolder(kind, "done"), null);
  }
  assert.equal(stepHolder("human", "waiting"), "you");
  assert.equal(stepHolder("approval", "waiting"), "you");
  assert.equal(stepHolder("wait", "waiting", "release"), "you");
  assert.equal(stepHolder("wait", "waiting", "signal"), "world");
  assert.equal(stepHolder("spawn", "waiting"), "world");
});

test("every catch the world moves waits on the world; only a release waits on you", () => {
  for (const until of variantsOf("workflow.rs", "WaitFor")) {
    assert.equal(stepHolder("wait", "waiting", until), until === "release" ? "you" : "world", until);
  }
  assert.equal(stepHolder("approval", "diverted"), null, "a diverted step holds nobody");
  assert.equal(stepHolder("emit", "running"), "agents", "an emit runs in the engine");
});

// The wire shape: the kind word beside the kind's own fields, as serde flattens it.
const step = (id, kind, extra = {}) => ({ id, name: id.toUpperCase(), kind, ...extra, then: [], join: "all", on_fail: { on_fail: "fail" }, retries: 0, max_visits: 3 });

test("a start and a parallel are no rows of work: a run reads from its first step of work", () => {
  const routed = {
    workflow: { steps: [step("begin", "start", { on: { event: "schedule", every: 3600 } }), step("fan", "parallel"), step("draft", "agent"), step("review", "approval")] },
    steps: { begin: { state: { state: "done" } }, fan: { state: { state: "done" } }, draft: { state: { state: "running" }, started_at: 10 } },
  };
  assert.deepEqual(progressRows(routed, { steps: [], current: ["draft"], reached: 0, total: 2 }, 20).map((r) => r.id), ["draft", "review"]);
  const ghost = { steps: [{ id: "begin", name: "Begin", kind: "start", state: { state: "pending" } }, { id: "a", name: "A", kind: "agent", state: { state: "pending" } }], current: [], reached: 0, total: 1 };
  assert.deepEqual(progressRows(null, ghost).map((r) => r.id), ["a"], "a ghosted plan too");
});
const run = {
  workflow: { steps: [step("draft", "agent"), step("review", "human", { options: [] }), step("ship", "approval"), step("end", "end")] },
  steps: {
    draft: { state: { state: "done" }, started_at: 100, finished_at: 160, work_item: "W1", output: { ok: true } },
    review: { state: { state: "waiting" }, started_at: 200 },
  },
};
const strip = { steps: [], current: ["review"], reached: 2, total: 4 };

test("rows follow the frozen workflow in order and carry state, holder, timing, output and verbs", () => {
  const rows = progressRows(run, strip, 260);
  assert.deepEqual(rows.map((r) => r.id), ["draft", "review", "ship", "end"]);
  assert.equal(rows[0].state.state, "done");
  assert.equal(rows[0].durationSecs, 60);
  assert.equal(rows[0].workItem, "W1");
  assert.deepEqual(rows[0].output, { ok: true });
  assert.equal(rows[0].holder, null);
  assert.equal(rows[1].holder, "you");
  assert.equal(rows[1].durationSecs, 60, "a live step counts to now");
  assert.deepEqual(rows[1].actions, ["answer", "done"]);
  assert.ok(rows[1].current);
  assert.equal(rows[2].state.state, "pending");
  assert.equal(rows[2].durationSecs, null);
  assert.deepEqual(rows[2].actions, []);
});

test("without a run the strip's definition ghosts as pending rows", () => {
  const ghost = { steps: [{ id: "a", name: "A", kind: "agent", state: { state: "pending" } }], current: [], reached: 0, total: 1 };
  const rows = progressRows(null, ghost);
  assert.equal(rows.length, 1);
  assert.equal(rows[0].state.state, "pending");
  assert.equal(rows[0].kind, "agent");
  assert.deepEqual(rows[0].actions, []);
});

test("the finished banner names the failed step and its reason, newest failure first", () => {
  const run = {
    outcome: "failed",
    workflow: { steps: [{ id: "build", name: "Build" }, { id: "review", name: "Review" }] },
    steps: {
      build: { state: { state: "failed" }, seq: 3, error: "old" },
      review: { state: { state: "failed" }, seq: 9, error: "{steps.build.output.summary} has no value in this run: step `build` failed — old" },
    },
  };
  assert.equal(finishedWords(run), "The run failed at Review — {steps.build.output.summary} has no value in this run: step `build` failed — old.");
  assert.equal(finishedWords({ outcome: "failed", workflow: { steps: [] }, steps: { x: { state: { state: "failed" }, seq: 1 } } }), "The run failed at x.", "the id when nothing names it, no dash without an error");
  assert.equal(finishedWords({ outcome: "failed", steps: {} }), "The run finished failed.");
  assert.equal(finishedWords({ outcome: "done", steps: {} }), "The run finished done.");
});

test("the count and the duration read as a person would say them", () => {
  assert.equal(progressCount(strip).label, "2 of 4 steps");
  assert.equal(progressCount({ reached: 0, total: 0 }).label, "no steps yet");
  // A row says how long its step took in the platform's one way of saying a span (`i18n/format.duration`).
  const took = (secs) => progressRows({ workflow: { steps: [step("a", "agent")] }, steps: { a: { state: { state: "done" }, started_at: 100, finished_at: 100 + secs } } }, { steps: [], current: [] }, 9999)[0].duration;
  assert.deepEqual([5, 125, 7200].map(took), ["5s", "2m", "2h"]);
  assert.equal(took(5400), "1h 30m", "an hour and a half is never rounded to two hours");
  assert.equal(took(93600), "1d 2h");
  const rows = progressRows(run, strip, 260);
  assert.deepEqual(rows.map((r) => r.duration), ["1m", "1m", null, null], "a step that never started says nothing");
  assert.ok(!readFileSync(join(HERE, "progressModel.mjs"), "utf8").includes("}s`"), "no unit is spelt in the model");
});

test("a spent step's row reads as a failed step with its error, and asks nothing", () => {
  const ring = {
    outcome: "done",
    workflow: { steps: [step("draft", "agent", { on_fail: { on_fail: "skip" } }), step("review", "human", { options: [], on_fail: { on_fail: "skip" } })] },
    steps: {
      draft: { state: { state: "failed" }, spent: true, visits: 3, seq: 11, started_at: 100, finished_at: 130, error: "entered 3 times; max_visits is 3", work_item: "W9" },
      review: { state: { state: "failed" }, spent: true, visits: 3, seq: 12, error: "entered 3 times; max_visits is 3" },
    },
  };
  const rows = progressRows(ring, { steps: [], current: [] }, 200);
  assert.deepEqual(
    rows.map((r) => [r.id, r.state.state, r.error, r.holder, r.visits, r.actions, r.current]),
    [
      ["draft", "failed", "entered 3 times; max_visits is 3", null, 3, ["open"], false],
      ["review", "failed", "entered 3 times; max_visits is 3", null, 3, [], false],
    ],
  );
  assert.equal(rows[0].duration, "30s");
  assert.equal(finishedWords(ring), "The run finished done.", "the ring passed over its failures: the run is done");
  assert.equal(finishedWords({ ...ring, outcome: "failed" }), "The run failed at REVIEW — entered 3 times; max_visits is 3.");
});

test("a live step's row stands open and a quiet one closed, until a person says otherwise", () => {
  const nothing = new Set();
  assert.equal(rowOpen(nothing, "build", true), true);
  assert.equal(rowOpen(nothing, "build", false), false);
  assert.equal(rowOpen(new Set(["build"]), "build", false), true, "opened by hand");
  assert.equal(rowOpen(new Set(["!build"]), "build", true), false, "closed by hand");
  assert.equal(rowOpen(new Set(["!build"]), "review", true), true, "a word about one step says nothing of another");
});

test("a row pressed changes how it stands, and keeps a word only where it says other than the row does on its own", () => {
  const nothing = new Set();
  // A live row closed by hand, then opened again: no word is left.
  const closed = rowPressed(nothing, "build", true);
  assert.deepEqual([...closed], ["!build"]);
  assert.equal(rowOpen(closed, "build", true), false);
  assert.deepEqual([...rowPressed(closed, "build", true)], []);
  // A quiet row opened by hand, then closed again: no word is left.
  const opened = rowPressed(nothing, "ship", false);
  assert.deepEqual([...opened], ["ship"]);
  assert.equal(rowOpen(opened, "ship", false), true);
  assert.deepEqual([...rowPressed(opened, "ship", false)], []);
  assert.equal(nothing.size, 0, "the set handed in is never written to");
});

test("a step that goes quiet closes by itself unless it was opened by hand, and one closed by hand stays closed when it goes live again", () => {
  assert.equal(rowOpen(new Set(), "build", false), false, "it was open only because it was live");
  const byHand = rowPressed(new Set(), "build", false);
  assert.equal(rowOpen(byHand, "build", true), true);
  assert.equal(rowOpen(byHand, "build", false), true, "opened by hand: it stays open through the run");
  // Opened by hand while quiet, pressed while live: closed, and said so.
  const pressed = rowPressed(byHand, "build", true);
  assert.deepEqual([...pressed], ["!build"]);
  assert.equal(rowOpen(pressed, "build", true), false);
  // The words survive the memory, which keeps them as a list.
  const kept = new Set(JSON.parse(JSON.stringify([...pressed])));
  assert.equal(rowOpen(kept, "build", true), false);
});

test("the live steps are the ones running or waiting, in definition order — what the list's popover acts on", () => {
  const run = {
    workflow: { steps: [{ id: "start" }, { id: "build" }, { id: "review" }, { id: "ship" }, { id: "tell" }] },
    steps: { start: { state: { state: "done" } }, build: { state: { state: "running" } }, review: { state: { state: "waiting" } }, ship: { state: { state: "pending" } } },
  };
  assert.deepEqual(liveSteps(run).map((s) => s.id), ["build", "review"]);
  for (const none of [null, undefined, {}, { workflow: { steps: [{ id: "a" }] } }]) assert.deepEqual(liveSteps(none), []);
  const popover = readFileSync(new URL("../_goals/GoalActPopover.tsx", import.meta.url), "utf8");
  assert.ok(popover.includes("const live = liveSteps(run);") && !popover.includes('=== "waiting"'), "the popover repeats no rule");
});
