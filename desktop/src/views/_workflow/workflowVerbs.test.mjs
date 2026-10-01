/**
 * The verbs a library workflow offers, and their words. Run with `npm test`
 * from `desktop/`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";

import { liveGoals, restartEveryWords, stopEveryWords, workflowVerbs } from "./workflowVerbs.mjs";

const goal = (id, live, label = id) => ({ kind: "goal", id, label, live });
const row = (over = {}) => ({ workflow: { id: "wf", archived: null }, problems: [], workspace_problems: [], runs: { live: 0, total: 0 }, used_by: [], ...over });
const starts = {
  manual: { step: "by-hand", event: "manual", summary: { id: "step-summary-start-manual" } },
  schedule: { step: "weekly", event: "schedule", summary: { id: "step-summary-start-cron", args: { cron: "0 9 * * 1" } } },
};

test("a workflow runs in the workspace when nothing stops it there; stop and restart need a run of it going", () => {
  assert.deepEqual(workflowVerbs(row()), { run: { label: "Run…", test: false }, stop: null, restart: null, turnOn: null, turnOff: null });
  assert.equal(workflowVerbs(row({ problems: [{ step: "a", kind: "unknown_step" }] })).run, null, "problems: nothing starts");
  assert.equal(workflowVerbs(row({ workspace_problems: [{ step: "a", kind: "needs_goal" }] })).run, null, "a step reads the goal: it runs on a goal only");
  assert.deepEqual(workflowVerbs(row({ workflow: { id: "wf", archived: { at: 1 } } })), { run: null, stop: null, restart: null, turnOn: null, turnOff: null });
  const busy = row({ runs: { live: 2, total: 5 } });
  assert.deepEqual(workflowVerbs(busy).stop, { label: "Stop every run", live: 2 });
  assert.deepEqual(workflowVerbs(busy).restart, { label: "Restart every run", live: 2 });
  const archivedBusy = workflowVerbs({ ...busy, workflow: { id: "wf", archived: { at: 1 } } });
  assert.ok(archivedBusy.stop, "an archived workflow's runs can still be stopped");
  assert.equal(archivedBusy.restart, null, "but not restarted");
  assert.deepEqual(workflowVerbs(null), { run: null, stop: null, restart: null, turnOn: null, turnOff: null });
});

test("a workflow only events begin runs by hand only as a test run", () => {
  const eventOnly = row({ starts: [starts.schedule], event_only: true });
  assert.deepEqual(workflowVerbs(eventOnly).run, { label: "Test run…", test: true });
  assert.deepEqual(workflowVerbs(row({ starts: [starts.manual, starts.schedule], event_only: false })).run, { label: "Run…", test: false }, "a start by hand beside it: a run by hand");
});

test("a workflow with an event start turns On when nothing is in the way, and Off once it is On", () => {
  const listens = row({ starts: [starts.manual, starts.schedule] });
  assert.deepEqual(workflowVerbs(listens).turnOn, { label: "Turn on…" });
  assert.equal(workflowVerbs(listens).turnOff, null);
  assert.equal(workflowVerbs(row({ starts: [starts.manual] })).turnOn, null, "nothing to hear");
  assert.equal(workflowVerbs({ ...listens, problems: [{ kind: "bad_timer" }] }).turnOn, null, "problems: it can't turn on");
  assert.equal(workflowVerbs({ ...listens, workflow: { id: "wf", archived: { at: 1 } } }).turnOn, null, "put away");
  assert.equal(workflowVerbs({ ...listens, workspace_problems: [{ step: "a", kind: "needs_goal" }] }).turnOn, null, "a step reads the goal: an event would start a run in the workspace, which has none");
  const on = workflowVerbs({ ...listens, listening: { since: 1 } });
  assert.equal(on.turnOn, null);
  assert.deepEqual(on.turnOff, { label: "Turn off" });
  assert.deepEqual(workflowVerbs({ ...listens, listening: { since: 1, paused: { reason: { reason: "budget_spent" }, at: 2 } } }).turnOff, { label: "Turn off" }, "a paused workflow still turns off");
});

test("a goal's run of the workflow is its goal's: it offers no stop and no restart here", () => {
  const onGoals = row({ used_by: [goal("g1", true, "Ship v2")] });
  const v = workflowVerbs(onGoals);
  assert.equal(v.stop, null);
  assert.equal(v.restart, null);
  assert.deepEqual(liveGoals(onGoals.used_by).map((g) => g.id), ["g1"], "the designer's freeze still reads it");
});

test("the confirms count the runs and say a goal's run goes on", () => {
  assert.equal(stopEveryWords(1), "The run of this workflow that is going is cancelled and its sessions end. A goal's run of it is its goal's, and goes on.");
  assert.equal(stopEveryWords(3), "The 3 runs of this workflow that are going are cancelled and their sessions end. A goal's run of it is its goal's, and goes on.");
  assert.equal(restartEveryWords(1), "The run of this workflow that is going is cancelled and a new run starts at once, with its inputs. A goal's run of it is its goal's, and goes on.");
  assert.equal(restartEveryWords(2), "The 2 runs of this workflow that are going are cancelled and a new run starts at once in place of each, with its inputs. A goal's run of it is its goal's, and goes on.");
});
