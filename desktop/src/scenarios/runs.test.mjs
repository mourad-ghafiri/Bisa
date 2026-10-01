/**
 * A goal's runs, as a person drives them: a start, a second that queues, a
 * stop that leaves a draft and an empty queue, a restart that queues a run
 * behind the new live one, a withdrawal offered on that row alone — the
 * verbs and the words a screen shows after each step, stepped through the
 * models the way the components do. No DOM: a wrong pixel is visible, a
 * wrong fact is not.
 *
 * Run with `node --test desktop/src/scenarios/runs.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { panelFrozen, queuedRuns, restartWords, runRows, runVerbs, runWords, stopWords } from "../views/_goal/runControl.mjs";
import { queuedChip, rowVerbs } from "../views/_goals/goalCardModel.mjs";
import { engineLine } from "../activityModel.mjs";

const goal = { id: "01G", mode: "manual", closed: null, workflow: "01W", run: null };
const guidance = { mode: "manual", design_enabled: true, phase: "manual", design: null, open_questions: [] };
const summary = (id, status, over = {}) => ({ id, status, workflow: "01W", revision: 1, queued_at: Number(id.slice(-1)), ...over });
const snapshot = (id, over = {}) => ({ id, started_at: 1, outcome: null, cancelled: null, steps: {}, ...over });
const verbsOf = (run, runs) => runVerbs({ goal: { ...goal, run: run?.id ?? null }, run, runs, guidance, proposed: false, startable: true });

test("start, queue, stop, restart, withdraw — the goal page and the list agree at every step", () => {
  // A draft with a workflow: Start run…, nothing to stop.
  let run = null;
  let runs = [];
  assert.deepEqual(verbsOf(run, runs), { start: { label: "Start run…", queues: false, adopt: false }, stop: null, restart: null });
  assert.equal(panelFrozen(run, runs), false);

  // Start: the first run is live and waits on a person; the panel freezes.
  run = snapshot("r1");
  runs = [summary("r1", "waiting", { started_at: 1 })];
  let v = verbsOf(run, runs);
  assert.equal(v.start.label, "New run…");
  assert.equal(v.start.queues, true, "a second run would queue");
  assert.deepEqual(v.stop, { label: "Stop", live: true, queued: 0 });
  assert.equal(panelFrozen(run, runs), true);
  let row = { id: goal.id, status: "waiting", holder: "you", workflow: "01W", run: "r1", run_status: "waiting", queued: 0 };
  assert.deepEqual(rowVerbs(row).stop, v.stop, "the list's card stops the same goal");
  assert.equal(queuedChip(row), null);

  // New run…: the second queues, next in line; the header still says waiting.
  runs = [summary("r2", "queued", { position: 1 }), ...runs];
  row = { ...row, queued: 1 };
  v = verbsOf(run, runs);
  assert.deepEqual(v.stop, { label: "Stop", live: true, queued: 1 });
  assert.equal(runWords(runs[0]).word, "queued · next in line");
  assert.equal(queuedChip(row), "queued 1");
  assert.deepEqual(runRows(runs).map((r) => [r.index, r.withdraw]), [[2, true], [1, false]], "the Runs list: run 2 queued and withdrawable, run 1 not");
  assert.equal(engineLine({ payload: { type: "run_queued", run: "r2", workflow: "01W", position: 1 } }, 1).text, "run r2 queued (next in line) on workflow 01W");
  assert.equal(panelFrozen(run, runs), true);

  // Stop: the confirm says what goes; after it, a draft with an empty queue.
  assert.equal(stopWords({ live: v.stop.live, queued: v.stop.queued, liveSteps: 1 }), "The live run is cancelled — 1 step is live and its work stops. 1 queued run is withdrawn. The goal stays open, ready for a new run.");
  run = snapshot("r1", { cancelled: { cause: "stopped", rationale: null } });
  runs = [summary("r2", "cancelled", { cause: { cause: "withdrawn" } }), summary("r1", "cancelled", { started_at: 1, finished_at: 5, cause: { cause: "stopped", rationale: null } })];
  row = { ...row, status: "draft", holder: "you", run_status: "cancelled", queued: 0 };
  v = verbsOf(run, runs);
  assert.deepEqual(v, { start: { label: "New run…", queues: false, adopt: false }, stop: null, restart: { label: "Restart" } });
  assert.equal(queuedRuns(runs).length, 0, "the queue is empty");
  assert.equal(panelFrozen(run, runs), false, "the panel opens again");
  assert.deepEqual(runRows(runs).map((r) => r.words.word), ["withdrawn", "stopped"]);
  assert.equal(engineLine({ payload: { type: "run_cancelled", run: "r1", cause: { cause: "stopped", rationale: null } } }, 1).text, "run r1 stopped");
  assert.equal(engineLine({ payload: { type: "run_cancelled", run: "r1", cause: { cause: "stopped", rationale: null } } }, 1).tone, "dim", "the Pulse never calls a stop a failure");
  assert.deepEqual(rowVerbs(row), v, "the list agrees");

  // Restart on the draft: no confirm — nothing is live — and a new live run.
  assert.equal(restartWords({ live: false, queued: 0 }), null);
  run = snapshot("r3");
  runs = [summary("r3", "waiting", { started_at: 7 }), ...runs];
  v = verbsOf(run, runs);
  assert.deepEqual(v.stop, { label: "Stop", live: true, queued: 0 });
  assert.equal(runRows(runs)[0].index, 3);

  // New run… while live: queued; a restart now confirms and keeps the queue's place.
  runs = [summary("r4", "queued", { position: 1 }), ...runs];
  v = verbsOf(run, runs);
  assert.match(restartWords({ live: v.stop.live, queued: v.stop.queued }), /the 1 queued run keeps its place behind it/);
  const rows = runRows(runs);
  assert.deepEqual(rows.filter((r) => r.withdraw).map((r) => r.id), ["r4"], "withdraw is offered on the queued row alone");
  assert.equal(rows.find((r) => r.id === "r3").withdraw, false);
  assert.equal(rows.find((r) => r.id === "r1").withdraw, false);
});
