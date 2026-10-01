/**
 * The run verbs a goal offers, and the words around them, tested where the
 * rule lives. Run with `npm test` from `desktop/`.
 */

import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import {
  FROZEN_HINT,
  anyRunLive,
  cancelWords,
  listenVerb,
  liveRun,
  panelFrozen,
  queuedRuns,
  restartWords,
  runIndex,
  runIsLive,
  runIsQueued,
  runRows,
  runStatus,
  runVerbs,
  runWords,
  stopWords,
  summaryIsLive,
  summaryIsQueued,
} from "./runControl.mjs";

const goal = { id: "01G", mode: "manual", closed: null };
const manual = { mode: "manual", design_enabled: true, phase: "manual", design: null, open_questions: [] };
const running = { id: "01R", started_at: 10, outcome: null, cancelled: null, steps: {} };
const queued = { id: "01Q", started_at: null, outcome: null, cancelled: null, steps: {} };
const done = { id: "01R", started_at: 10, outcome: "done", cancelled: null, steps: {} };
const failed = { id: "01R", started_at: 10, outcome: "failed", cancelled: null, steps: {} };
const stopped = { id: "01R", started_at: 10, outcome: null, cancelled: { cause: "stopped", rationale: null }, steps: {} };
const withdrawn = { id: "01Q", started_at: null, outcome: null, cancelled: { cause: "withdrawn" }, steps: {} };
const s = (id, status, over = {}) => ({ id, status, workflow: "01W", revision: 1, queued_at: Number(id.replace(/\D/g, "")) || 1, ...over });
const args = (over) => ({ goal, run: null, runs: [], guidance: manual, proposed: false, startable: true, ...over });
const none = { start: null, stop: null, restart: null };

test("a design that begins on events starts by listening; once the goal listens, a run by hand is Run now at its start by hand", () => {
  // Not listening yet: the start arms the design's start events.
  assert.deepEqual(runVerbs(args({ listens: true, manualEntry: "by-hand" })).start, { label: "Start listening…", queues: false, adopt: false, listen: true });
  assert.deepEqual(runVerbs(args({ listens: true, manualEntry: null })).start, { label: "Start listening…", queues: false, adopt: false, listen: true }, "an event-only design listens too");
  // Listening: a run by hand at its start by hand — queued behind a live run — or none when only events begin it.
  const listening = { ...goal, listening: { since: 5 } };
  assert.deepEqual(runVerbs(args({ goal: listening, listens: true, manualEntry: "by-hand" })).start, { label: "Run now…", queues: false, adopt: false, at: "by-hand" });
  assert.deepEqual(runVerbs(args({ goal: listening, listens: true, manualEntry: "by-hand", run: running, runs: [s("1", "running")] })).start, { label: "Run now…", queues: true, adopt: false, at: "by-hand" });
  assert.equal(runVerbs(args({ goal: listening, listens: true, manualEntry: null })).start, null, "only events begin it: nothing to run by hand");
  // A proposal still adopts first; a design by hand is unchanged.
  assert.equal(runVerbs(args({ proposed: true, listens: true })).start.adopt, true);
  assert.deepEqual(runVerbs(args()).start, { label: "Start run…", queues: false, adopt: false });
});

test("a listening goal's line offers Stop listening, and Listen again once a failure paused it", () => {
  assert.equal(listenVerb(goal), null, "a goal that does not listen");
  assert.deepEqual(listenVerb({ ...goal, listening: { since: 1 } }), { id: "stop", label: "Stop listening" });
  assert.deepEqual(listenVerb({ ...goal, listening: { since: 1, paused: { reason: { reason: "run_failed", run: "01R" }, at: 2 } } }), { id: "again", label: "Listen again" });
  assert.equal(listenVerb({ ...goal, listening: { since: 1 }, closed: { reason: "abandoned" } }), null, "a closed goal listens to nothing");
  assert.equal(listenVerb(null), null);
});

test("a run is live once started and until it has an outcome or is cancelled; queued until it starts", () => {
  assert.equal(runIsLive(running), true);
  assert.equal(runIsLive(queued), false, "a queued run is not live");
  assert.equal(runIsQueued(queued), true);
  assert.equal(runIsQueued(running), false);
  assert.equal(runIsQueued(withdrawn), false, "withdrawn is over, not queued");
  for (const r of [done, failed, stopped, withdrawn, null]) assert.equal(runIsLive(r), false);
  assert.equal(summaryIsLive({ status: "running" }), true);
  assert.equal(summaryIsLive({ status: "waiting" }), true);
  assert.equal(summaryIsLive({ status: "queued" }), false, "queued is not live");
  assert.equal(summaryIsQueued({ status: "queued" }), true);
  for (const st of ["done", "failed", "cancelled"]) assert.equal(summaryIsLive({ status: st }), false, st);
  assert.equal(anyRunLive(done, [s("1", "done"), s("2", "waiting")]), true, "the list is the safety net");
  assert.equal(anyRunLive(done, [s("1", "done"), s("2", "queued")]), false, "a queue alone is not live");
  assert.equal(liveRun(done, [s("1", "done"), s("2", "waiting")]), "2");
  assert.equal(liveRun(running, []), "01R");
  assert.equal(liveRun(done, [s("1", "done")]), null);
});

test("the queue is read in start order and a run's number is its place from the oldest", () => {
  const runs = [s("3", "queued", { position: 2 }), s("2", "queued", { position: 1 }), s("1", "waiting")];
  assert.deepEqual(queuedRuns(runs).map((r) => r.id), ["2", "3"]);
  assert.equal(runIndex(runs, "1"), 1);
  assert.equal(runIndex(runs, "3"), 3);
  assert.equal(runIndex(runs, "nope"), null);
  assert.deepEqual(runRows(runs).map((r) => [r.id, r.index, r.withdraw]), [["3", 3, true], ["2", 2, true], ["1", 1, false]], "newest first; withdraw on the queued rows alone");
  assert.equal(runRows(runs)[0].words.word, "queued · 2nd in line");
  assert.deepEqual(runRows(runs).map((r) => r.live), [true, true, true], "a row that is going or waiting its turn is never folded away by a list's bound");
  assert.deepEqual(runRows([s("2", "done"), s("1", "cancelled")]).map((r) => r.live), [false, false]);
});

test("the verb matrix: draft, live, queued, finished, closed, designing, adopt, stalled", () => {
  // A draft with a workflow: Start run…, nothing to stop or restart.
  assert.deepEqual(runVerbs(args()), { start: { label: "Start run…", queues: false, adopt: false }, stop: null, restart: null });
  // Live: a new run queues, the goal can stop and restart.
  assert.deepEqual(runVerbs(args({ run: running, runs: [s("1", "running")] })), {
    start: { label: "New run…", queues: true, adopt: false },
    stop: { label: "Stop", live: true, queued: 0 },
    restart: { label: "Restart" },
  });
  // Live with a queue: the stop counts the queue.
  assert.deepEqual(runVerbs(args({ run: running, runs: [s("2", "queued", { position: 1 }), s("1", "running")] })).stop, { label: "Stop", live: true, queued: 1 });
  // Only a queue, nothing live (the moment between a run's end and the next start): a stop withdraws it; a new run starts now.
  assert.deepEqual(runVerbs(args({ run: done, runs: [s("2", "queued", { position: 1 }), s("1", "done")] })), {
    start: { label: "New run…", queues: false, adopt: false },
    stop: { label: "Stop", live: false, queued: 1 },
    restart: { label: "Restart" },
  });
  // Finished: New run… and Restart, no stop.
  for (const r of [done, failed, stopped]) {
    assert.deepEqual(runVerbs(args({ run: r, runs: [s("1", r.outcome ?? "cancelled")] })), {
      start: { label: "New run…", queues: false, adopt: false },
      stop: null,
      restart: { label: "Restart" },
    });
  }
  // A proposal: Adopt and start…, never a restart (nothing ran).
  assert.deepEqual(runVerbs(args({ proposed: true })), { start: { label: "Adopt and start…", queues: false, adopt: true }, stop: null, restart: null });
  // Closed, or no workflow: nothing, or nothing to start.
  assert.deepEqual(runVerbs(args({ goal: { ...goal, closed: { reason: "abandoned" } }, run: running })), none);
  assert.deepEqual(runVerbs(args({ startable: false, run: done })), { start: null, stop: null, restart: null });
  assert.deepEqual(runVerbs(args({ startable: false, run: running, runs: [s("1", "running")] })).stop, { label: "Stop", live: true, queued: 0 }, "a stop needs no workflow");
  // The Workflow Agent at work blocks a start and a restart, never a stop.
  const guided = { ...goal, mode: "guided" };
  const designing = { mode: "guided", design_enabled: true, phase: "design", design: { phase: "design", status: "working", since: 1, detail: null, session: null, live: true }, open_questions: [] };
  assert.deepEqual(runVerbs(args({ goal: guided, guidance: designing })), none, "designing");
  const repairing = { ...designing, phase: "repair", design: { ...designing.design, phase: "repair" } };
  assert.deepEqual(runVerbs(args({ goal: guided, guidance: repairing, run: failed, runs: [s("1", "failed")] })), none, "a failed run under repair is not restarted over the agent's head");
  const stalled = { ...designing, design: { ...designing.design, status: "stalled", live: false } };
  assert.deepEqual(runVerbs(args({ goal: guided, guidance: stalled, run: failed, runs: [s("1", "failed")] })), {
    start: { label: "New run…", queues: false, adopt: false },
    stop: null,
    restart: { label: "Restart" },
  }, "once the agent stopped, a person may");
});

test("the words: a run's status, a cancel's cause, a stop's and a restart's consequences", () => {
  assert.deepEqual(runWords(s("1", "queued", { position: 1 })), { word: "queued · next in line", tone: "quiet", at: 1, atWord: "queued" });
  assert.equal(runWords(s("3", "queued", { position: 3 })).word, "queued · 3rd in line");
  assert.deepEqual(runWords(s("1", "running", { started_at: 5 })), { word: "running", tone: "accent", at: 5, atWord: "started" });
  assert.equal(runWords(s("1", "waiting", { started_at: 5 })).tone, "warn");
  assert.deepEqual(runWords(s("1", "done", { started_at: 5, finished_at: 9 })), { word: "done", tone: "ok", at: 9, atWord: "finished" });
  assert.equal(runWords(s("1", "failed", { finished_at: 9 })).tone, "danger");
  assert.deepEqual(runWords(s("1", "cancelled", { finished_at: 9, cause: { cause: "stopped", rationale: "enough" } })), { word: "stopped", tone: "quiet", at: 9, atWord: "ended" });
  assert.equal(runWords(s("1", "cancelled", { cause: { cause: "withdrawn" } })).word, "withdrawn");
  assert.equal(runWords(s("1", "cancelled", { cause: { cause: "restarted" } })).word, "restarted");
  assert.equal(runWords(s("1", "cancelled", { cause: { cause: "closed", reason: { reason: "abandoned" } } })).word, "closed");
  assert.equal(runWords(s("1", "cancelled", { cause: { cause: "retired" } })).word, "retired", "a run of the workspace whose workflow was archived or deleted while it went");
  assert.equal(cancelWords(null), "cancelled");
  // A place's ending is the language's rule, chosen in the message (`ORDINAL`) — no English suffix is built in code.
  assert.deepEqual(
    [2, 3, 4, 11, 12, 13, 21, 22, 101].map((position) => runWords(s("9", "queued", { position })).word),
    ["2nd", "3rd", "4th", "11th", "12th", "13th", "21st", "22nd", "101st"].map((place) => `queued · ${place} in line`),
  );
  assert.ok(!readFileSync(new URL("./runControl.mjs", import.meta.url), "utf8").includes('"th"'), "the ending is the catalog's");

  assert.equal(stopWords({ live: true, queued: 0, liveSteps: 0 }), "The live run is cancelled. The goal stays open, ready for a new run.");
  assert.equal(stopWords({ live: true, queued: 2, liveSteps: 1 }), "The live run is cancelled — 1 step is live and its work stops. 2 queued runs are withdrawn. The goal stays open, ready for a new run.");
  assert.equal(stopWords({ live: false, queued: 1 }), "1 queued run is withdrawn. The goal stays open, ready for a new run.");
  assert.equal(restartWords({ live: false, queued: 0 }), null, "nothing live: no confirm");
  assert.equal(restartWords({ live: true, queued: 0 }), "The live run is cancelled and a new run of the same workflow starts at once with the same inputs.");
  assert.match(restartWords({ live: true, queued: 2 }), /the 2 queued runs keep their place behind it/);
  assert.match(restartWords({ live: true, queued: 1 }), /the 1 queued run keeps its place/);
});

test("the right panel is frozen exactly while a run is live — a queue freezes nothing", () => {
  assert.equal(panelFrozen(running, []), true);
  assert.equal(panelFrozen(done, [s("1", "done"), s("2", "waiting")]), true, "a live run in the list, whatever the pointer says");
  assert.equal(panelFrozen(done, [s("1", "done"), s("2", "queued")]), false, "a queue alone freezes nothing");
  assert.equal(panelFrozen(done, [s("1", "done")]), false);
  assert.equal(panelFrozen(failed, [s("1", "failed")]), false, "a failed run is over: the panel opens again");
  assert.equal(panelFrozen(stopped, [s("1", "cancelled")]), false);
  assert.equal(panelFrozen(null, []), false);
  assert.match(FROZEN_HINT, /read-only/);
  assert.match(FROZEN_HINT, /workflow/, "the workflow is frozen with the rest");
  assert.doesNotMatch(FROZEN_HINT, /Amend/, "no door into a running workflow is offered");
});

test("a run's status is the core's projection: running while any step runs, waiting while every live step waits", () => {
  // The rule restated (`WorkflowRun::status`), held to the source.
  const core = readFileSync(new URL("../../../../crates/bisa-core/src/run.rs", import.meta.url), "utf8");
  const rule = core.match(/pub fn status\(&self\) -> RunStatus \{([\s\S]*?)\n {4}\}/);
  assert.ok(rule, "WorkflowRun::status in run.rs");
  const said = rule[1].replace(/\s+/g, " ");
  assert.ok(said.includes("if self.cancelled.is_some() { return RunStatus::Cancelled; }"), "a cancel outranks an outcome");
  assert.ok(said.includes("None if self.started_at.is_none() => RunStatus::Queued"));
  assert.ok(said.includes("any(|r| r.state == StepState::Running) { RunStatus::Running } else { RunStatus::Waiting }"));

  const live = (steps) => ({ ...running, steps });
  assert.equal(runStatus(live({ a: { state: { state: "done" } }, b: { state: { state: "running" } } })), "running");
  assert.equal(runStatus(live({ a: { state: { state: "done" } }, b: { state: { state: "waiting" } } })), "waiting", "its one live step waits on a person: the run waits");
  assert.equal(runStatus(live({ a: { state: { state: "waiting" } }, b: { state: { state: "running" } } })), "running", "one step still runs");
  assert.equal(runStatus(live({})), "waiting", "nothing runs: between two events the run waits");
  assert.equal(runStatus(queued), "queued");
  assert.equal(runStatus(done), "done");
  assert.equal(runStatus(failed), "failed");
  assert.equal(runStatus(stopped), "cancelled");
  assert.equal(runStatus({ ...done, cancelled: { cause: "retired" } }), "cancelled", "a cancel outranks an outcome");
  assert.equal(runStatus(null), null);
});
