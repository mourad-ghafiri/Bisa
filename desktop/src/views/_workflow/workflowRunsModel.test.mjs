/**
 * A workflow's runs of the workspace, as the Runs pane and a run's page read
 * them. Run with `npm test` from `desktop/`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";

import { RUNS_SHOWN, inFlight, isLive, movesRun, readStanding, movesRunCount, movesRunsOf, orderRuns, paneWindow, runPageFacts, runRowVerbs, runTitle, runsPaneRows, settled, startedBy, taken } from "./workflowRunsModel.mjs";

const run = (id, status, over = {}) => ({ id, status, scope: "workspace", number: 1, workflow: "wf", workflow_name: "Nightly report", revision: 1, queued_at: 1, ...over });

test("the runs still going come first, then the rest, each group newest first as the node answered", () => {
  const newestFirst = [run("r5", "done"), run("r4", "waiting"), run("r3", "failed"), run("r2", "running"), run("r1", "cancelled")];
  assert.deepEqual(orderRuns(newestFirst).map((r) => r.id), ["r4", "r2", "r5", "r3", "r1"]);
  assert.deepEqual(orderRuns(null), []);
  for (const s of ["queued", "running", "waiting"]) assert.ok(isLive(s), s);
  for (const s of ["done", "failed", "cancelled"]) assert.ok(!isLive(s), s);
});

test("a run is titled by its workflow and its number, and says who started it", () => {
  assert.equal(runTitle(run("r3", "done", { number: 3 })), "Nightly report #3");
  assert.equal(startedBy(run("r1", "done")), "by you", "a summary that says nothing is a person's");
  assert.equal(startedBy(run("r1", "done", { started_by: { by: "you" } })), "by you");
  assert.equal(startedBy(run("r1", "done", { started_by: { by: "test", event: "hook" } })), "test run");
});

test("a run an event started says which kind of event, and the detail the node gives", () => {
  const by = (event, detail) => startedBy(run("r1", "done", { started_by: detail === undefined ? { by: "event", event } : { by: "event", event, detail } }));
  assert.deepEqual(
    ["schedule", "hook", "message", "signal", "project", "run", "platform", "connector", "check"].map((e) => by(e)),
    ["by schedule", "by hook", "by message", "by signal", "by project change", "by run", "by platform event", "by connector", "by check"],
  );
  assert.equal(by("message", "Maya"), "by message from Maya");
  assert.equal(by("signal", "report.ready"), "by signal report.ready");
  assert.equal(by("run", "#4"), "by run #4");
  assert.equal(by("message", ""), "by message", "an empty detail is none");
  assert.equal(by("schedule", "Mon 09:00"), "by schedule", "a kind that takes no detail says none");
  assert.equal(by("carrier_pigeon"), "by an event", "a kind this build has no word for");
  assert.equal(by("toString"), "by an event", "nor a word that only looks like one");
  assert.equal(startedBy(run("r1", "done", { trigger: "01J00000000000000000TRIG42" })), "by you", "the retired trigger field is not read");
});

test("a run a start event began reads as the plan's words: by schedule, by message from Maya, by signal report.ready, by run #4, test", () => {
  const rows = runsPaneRows([
    run("r7", "done", { number: 7, started_by: { by: "test", event: "signal" } }),
    run("r6", "done", { number: 6, started_by: { by: "event", event: "run", detail: "#4" } }),
    run("r5", "done", { number: 5, started_by: { by: "event", event: "signal", detail: "report.ready" } }),
    run("r4", "done", { number: 4, started_by: { by: "event", event: "message", detail: "Maya" } }),
    run("r3", "done", { number: 3, started_by: { by: "event", event: "hook" } }),
    run("r2", "done", { number: 2, started_by: { by: "event", event: "schedule" } }),
    run("r1", "done", { number: 1, started_by: { by: "you" } }),
  ]);
  assert.deepEqual(rows.map((r) => r.startedBy), ["test run", "by run #4", "by signal report.ready", "by message from Maya", "by hook", "by schedule", "by you"]);
});

test("a run of the workspace stops while it goes and restarts any time; a goal's run offers neither here", () => {
  assert.deepEqual(runRowVerbs(run("r1", "waiting")), { stop: true, restart: true, open: true });
  assert.deepEqual(runRowVerbs(run("r1", "done")), { stop: false, restart: true, open: true });
  assert.deepEqual(runRowVerbs(run("r1", "running", { scope: "goal", goal: "g1" })), { stop: false, restart: false, open: true });
});

test("the pane's rows carry the title, the words, who started it and the verbs", () => {
  const rows = runsPaneRows([run("r2", "done", { number: 2, finished_at: 9 }), run("r1", "waiting", { number: 1, started_at: 5 })]);
  assert.deepEqual(rows.map((r) => [r.id, r.title, r.words.word, r.live]), [
    ["r1", "Nightly report #1", "waiting", true],
    ["r2", "Nightly report #2", "done", false],
  ]);
  assert.equal(rows[0].words.at, 5);
  assert.equal(rows[1].startedBy, "by you");
  const retired = runsPaneRows([run("r9", "cancelled", { cause: { cause: "retired" } })]);
  assert.equal(retired[0].words.word, "retired", "a run its workflow's retirement ended says so");
});

test("a run of this workflow moving is what refreshes the pane; another workflow's, or another fact, is not", () => {
  assert.ok(movesRunsOf({ workflow: "wf", payload: { type: "step_changed" } }, "wf"));
  assert.ok(movesRunsOf({ workflow: "wf", payload: { type: "run_finished" } }, "wf"));
  assert.ok(movesRunsOf({ workflow: "wf", payload: { type: "boundary_fired", diverts: true } }, "wf"), "a timeout that diverted one of its steps");
  assert.ok(!movesRunsOf({ workflow: "other", payload: { type: "run_finished" } }, "wf"));
  assert.ok(!movesRunsOf({ workflow: "wf", payload: { type: "gate_opened" } }, "wf"));
  assert.ok(!movesRunsOf({ payload: { type: "run_started" } }, "wf"), "a goal-less envelope with no workflow is nobody's here");
});

test("an archived workflow's runs stop and open, and none restarts: the node would refuse the new run", () => {
  assert.deepEqual(runRowVerbs(run("r1", "waiting"), { archived: true }), { stop: true, restart: false, open: true });
  assert.deepEqual(runRowVerbs(run("r1", "done"), { archived: true }), { stop: false, restart: false, open: true });
  assert.deepEqual(runRowVerbs(run("r1", "done"), { archived: false }), { stop: false, restart: true, open: true });
  const rows = runsPaneRows([run("r2", "running", { number: 2 }), run("r1", "done", { number: 1 })], { archived: true });
  assert.deepEqual(rows.map((r) => [r.id, r.verbs.stop, r.verbs.restart]), [["r2", true, false], ["r1", false, false]]);
});

test("the pane draws a bounded list: every run still going, then the newest, and says how many it left out", () => {
  const many = runsPaneRows(Array.from({ length: 130 }, (_, i) => run(`r${130 - i}`, i === 129 ? "waiting" : "done", { number: 130 - i })));
  assert.equal(many[0].id, "r1", "the one still going leads, however old");
  const first = paneWindow(many);
  assert.deepEqual([first.rows.length, first.hidden, first.more], [RUNS_SHOWN, 130 - RUNS_SHOWN, RUNS_SHOWN]);
  assert.equal(first.rows[0].id, "r1");
  const second = paneWindow(many, RUNS_SHOWN * 2);
  assert.deepEqual([second.rows.length, second.hidden, second.more], [100, 30, 30], "the last page is what is left");
  assert.deepEqual([paneWindow(many, 500).rows.length, paneWindow(many, 500).hidden, paneWindow(many, 500).more], [130, 0, 0]);
  // A live run is never folded away, whatever the bound.
  const busy = runsPaneRows(Array.from({ length: 60 }, (_, i) => run(`r${60 - i}`, "running", { number: 60 - i })));
  assert.deepEqual([paneWindow(busy).rows.length, paneWindow(busy).hidden], [60, 0]);
  assert.deepEqual(paneWindow([]), { rows: [], hidden: 0, more: 0 });
  assert.deepEqual(paneWindow(many, 0).rows.length, RUNS_SHOWN, "a bound that is none is the default");
  assert.deepEqual(paneWindow(many, Number.NaN).rows.length, RUNS_SHOWN);
});

test("a run's page moves on the facts about its own run — a gate or a question on it too — and leaves when its workflow goes", () => {
  const about = (type, over = {}) => ({ workflow: "wf", run: "r1", payload: { type, ...over } });
  for (const type of ["run_started", "run_finished", "run_cancelled", "step_changed", "boundary_fired"]) assert.ok(movesRun(about(type), "r1", "wf"), type);
  // What the run owes a person moves with its gates: a guard's question, an agent's, a decision made elsewhere.
  for (const type of ["gate_opened", "gate_decided", "question_asked"]) assert.ok(movesRun(about(type), "r1", "wf"), type);
  assert.ok(!movesRun({ workflow: "wf", run: "r2", payload: { type: "step_changed" } }, "r1", "wf"), "another run of the workflow is its own page's");
  assert.ok(!movesRun({ workflow: "wf", payload: { type: "gate_opened" } }, "r1", "wf"), "a gate that names no run is not this run's");
  assert.ok(!movesRun(about("session_state"), "r1", "wf"), "a session's heartbeat redraws nothing here");
  // Its workflow deleted takes its runs with it: the page reads again and finds nothing.
  assert.ok(movesRun({ payload: { type: "workflow_deleted", workflow: "wf" } }, "r1", "wf"));
  assert.ok(!movesRun({ payload: { type: "workflow_deleted", workflow: "other" } }, "r1", "wf"));
  assert.ok(!movesRun({ payload: { type: "workflow_archived", workflow: "wf", archived: true } }, "r1", "wf"), "archived, its runs stay as its history");
  assert.ok(!movesRun(about("step_changed"), null, null), "nothing read yet: nothing to move");
  assert.ok(!movesRun(null, "r1", "wf"));
});

test("a workflow's row counts its runs: one that starts or ends moves it, a step inside one does not", () => {
  for (const type of ["run_started", "run_finished", "run_cancelled"]) assert.ok(movesRunCount({ workflow: "wf", run: "r1", payload: { type } }, "wf"), type);
  assert.ok(movesRunCount({ workflow: "wf", goal: "g1", run: "r9", payload: { type: "run_started" } }, "wf"), "a goal's run of it too: that is what freezes the designer");
  assert.ok(!movesRunCount({ workflow: "wf", run: "r1", payload: { type: "step_changed" } }, "wf"));
  assert.ok(!movesRunCount({ workflow: "other", run: "r1", payload: { type: "run_started" } }, "wf"));
  assert.ok(!movesRunCount({ payload: { type: "run_finished" } }, "wf"));
});

test("the run's page decides before it draws: the header's words, the verbs, what is live, how it ended, and a goal's run handed on", () => {
  const workflow = { id: "wf", name: "Nightly report", revision: 3, steps: [{ id: "draft", name: "Draft", kind: "agent" }, { id: "review", name: "Review", kind: "human" }] };
  const view = (over = {}, summary = {}) => ({
    run: { id: "r3", workflow, started_at: 5, outcome: null, cancelled: null, steps: { draft: { state: { state: "done" } }, review: { state: { state: "waiting" } } }, ...over },
    summary: run("r3", "waiting", { number: 3, started_at: 5, started_by: { by: "event", event: "schedule" }, revision: 3, ...summary }),
    holder: "you",
    needs_actions: [{ gate_id: "g1", kind: "step", step: "review" }],
  });
  const going = runPageFacts(view());
  assert.deepEqual(
    { title: going.title, word: going.words.word, tone: going.words.tone, holder: going.holder, startedBy: going.startedBy, revision: going.revision, verbs: going.verbs, finished: going.finished, live: going.live, ended: going.ended, handsTo: going.handsTo },
    { title: "Nightly report #3", word: "waiting", tone: "warn", holder: "your move", startedBy: "by schedule", revision: "rev 3", verbs: { stop: true, restart: true, open: true }, finished: false, live: ["review"], ended: null, handsTo: null },
  );
  // Failed at a spent step: the line under the rows names it and why.
  const failed = runPageFacts(
    view({ outcome: "failed", finished_at: 9, steps: { draft: { state: { state: "failed" }, spent: true, seq: 7, error: "entered 3 times; max_visits is 3" } } }, { status: "failed", finished_at: 9 }),
  );
  assert.deepEqual([failed.finished, failed.live, failed.verbs.stop, failed.verbs.restart], [true, [], false, true]);
  assert.equal(failed.ended, "The run failed at Draft — entered 3 times; max_visits is 3.");
  const stopped = runPageFacts(view({ cancelled: { cause: "stopped", rationale: null }, finished_at: 9 }, { status: "cancelled", cause: { cause: "stopped", rationale: null }, finished_at: 9 }));
  assert.equal(stopped.finished, true);
  assert.match(stopped.ended, /stopped/);
  // A goal's run opened by its id is its goal's to show, on that run; it offers no verb here.
  const goals = runPageFacts(view({}, { scope: "goal", goal: "g1" }));
  assert.deepEqual(goals.handsTo, { route: { name: "goal", id: "g1" }, search: { tab: "workflow", run: "r3" } });
  assert.deepEqual(goals.verbs, { stop: false, restart: false, open: true });
  assert.equal(runPageFacts(null), null, "nothing read yet");
});

test("one verb at a time on a run: a second press while the first is on its way starts nothing", () => {
  // Restart pressed twice would start two runs: the second press finds the run taken.
  const none = new Set();
  const first = taken(none, "r1");
  assert.ok(first, "the first press is taken");
  assert.deepEqual([...first], ["r1"]);
  assert.equal(taken(first, "r1"), null, "the second press of the same run is refused");
  assert.equal(none.size, 0, "the set handed in is never written to");
  // Another run is its own: its verb goes beside the first.
  const both = taken(first, "r2");
  assert.deepEqual([...both].sort(), ["r1", "r2"]);
  assert.ok(inFlight(both, "r1") && inFlight(both, "r2") && !inFlight(both, "r3"));
  // Once the node answered — done or refused — the run takes a verb again.
  const after = settled(both, "r1");
  assert.deepEqual([...after], ["r2"]);
  assert.ok(taken(after, "r1"));
  assert.equal(settled(after, "r9"), after, "a run with nothing on its way changes nothing");
});

test("a read that failed keeps the last answer on screen and says so; only a run that is gone, or never read, shows the note alone", () => {
  const view = { run: {}, summary: {} };
  assert.deepEqual(readStanding({ data: null, loading: true, error: null, missing: false }, "the run"), { standing: "reading", line: null });
  assert.deepEqual(readStanding({ data: view, loading: false, error: null, missing: false }, "the run"), { standing: "ready", line: null });
  assert.deepEqual(readStanding({ data: null, loading: false, error: "the node is unreachable", missing: false }, "the run"), { standing: "failed", line: "the node is unreachable" });
  // A re-read refused — the node away for a moment, a deadline passed: the run as it was last read stays, with one line.
  assert.deepEqual(readStanding({ data: view, loading: false, error: "no answer within 10 s", missing: false }, "the run"), {
    standing: "stale",
    line: "could not read the run: no answer within 10 s — showing the last answer",
  });
  // Gone — its workflow was deleted: what the window kept of it is not drawn over the node's word.
  assert.deepEqual(readStanding({ data: view, loading: false, error: "no such run", missing: true }, "the run"), { standing: "gone", line: "no such run" });
  assert.deepEqual(readStanding({ data: view, loading: false, error: null, refreshing: true, missing: false }, "the runs"), { standing: "ready", line: null }, "a re-read on its way says nothing here");
});
