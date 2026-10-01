/**
 * A workflow run on its own (03-workflows §Runs of the workspace), as a
 * person drives it from the library: *Run…* on the card with no goal
 * asked, a workflow that reads its goal refused by name, two runs going at
 * once, the Runs pane while one of them ends, the run's page with what it
 * owes and the step whose visits ran out, an ask answered from the Inbox's
 * workflow row, one run stopped, *Stop every run*, a workflow that kept
 * every run it ever made, then the workflow retired. Every step reads the
 * same models the screens draw from; no DOM. Run with
 * `node --test --import ./desktop/src/i18n/preload.mjs desktop/src/scenarios/workspaceRuns.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { href, parse, section } from "../routeModel.mjs";
import { doorOf } from "../views/_studio/inboxModel.mjs";
import { attachedTo } from "../views/rosterModel.mjs";
import { historyLine, retireSections, runsLine } from "../views/_work/retireModel.mjs";
import { progressRows, rowOpen } from "../views/_goal/progressModel.mjs";
import { libraryReads } from "../views/_workflow/libraryModel.mjs";
import { switchState } from "../views/_workflow/listeningModel.mjs";
import { BY_HAND, askedInputs, firstEntry, runEntries, runRefusal, runRequest } from "../views/_workflow/runDialogModel.mjs";
import { edgeTone, stepLabel, stepTone } from "../views/_workflow/runView.mjs";
import { cardMenu, cardStatus } from "../views/_workflow/workflowCardModel.mjs";
import { restartEveryWords, stopEveryWords, workflowVerbs } from "../views/_workflow/workflowVerbs.mjs";
import { RUNS_SHOWN, inFlight, movesRun, movesRunCount, movesRunsOf, paneWindow, runPageFacts, runRowVerbs, runsPaneRows, settled, taken } from "../views/_workflow/workflowRunsModel.mjs";
import { PROBLEM_KIND_LABEL } from "../views/_workflow/stepKinds.mjs";
import { toGraph } from "../views/_workflow/workflowGraph.mjs";

const src = (name) => readFileSync(new URL(name, import.meta.url), "utf8");

const steps = [
  { id: "begin", name: "By hand", kind: "start", on: { event: "manual" }, then: [{ to: "draft" }] },
  { id: "draft", name: "Draft", kind: "agent", instructions: "Write the report for {inputs.audience}.", on_fail: { on_fail: "skip" }, max_visits: 3, then: [{ to: "review" }] },
  { id: "review", name: "Review", kind: "human", prompt: "Good to send?", options: [{ id: "yes", label: "Yes" }, { id: "again", label: "Again" }], then: [{ to: "send" }] },
  { id: "send", name: "Send", kind: "notify", template: "Sent.", then: [] },
];
const workflow = { id: "01WF", name: "Nightly report", description: "Every night's numbers.", archived: null, revision: 3, steps, inputs: [{ name: "audience", label: "Audience", kind: "text", required: true }], origin: { origin: "workspace" } };
const row = (over = {}) => ({ workflow, problems: [], workspace_problems: [], used_by: [], runs: { live: 0, total: 0 }, starts: [{ step: "begin", event: "manual", summary: { id: "step-summary-start-manual" } }], event_only: false, listening: null, listening_needs: [], ...over });
const summary = (id, status, number, over = {}) => ({ id, status, number, scope: "workspace", workflow: "01WF", workflow_name: "Nightly report", revision: 3, queued_at: number, started_at: number, started_by: { by: "you" }, ...over });
const fact = (type, run, over = {}) => ({ workflow: "01WF", run, payload: { type, run, ...over } });

test("Run… on the card starts a run in the workspace: no goal is asked, and the dialog lands on the run's page", () => {
  const ready = row();
  const verbs = workflowVerbs(ready);
  assert.ok(verbs.run, "a workflow with no problems runs in the workspace");
  assert.equal(verbs.stop, null, "nothing to stop before a run goes");
  assert.deepEqual(cardMenu(verbs).map((i) => i.id), ["open", "run", "delete"]);
  assert.equal(cardStatus(ready).words, "ready to run");

  // The dialog: one way in, by hand; it asks the workflow's inputs and nothing else.
  assert.deepEqual(runEntries(workflow).map((e) => e.id), [BY_HAND]);
  assert.equal(firstEntry(workflow), BY_HAND);
  assert.deepEqual(askedInputs(workflow, BY_HAND), { asked: ["audience"], mapped: 0 });
  assert.deepEqual(runRequest(workflow, BY_HAND, { audience: "" }, ""), { kind: "refused", errors: { audience: "Required." }, payloadError: null }, "a required input left blank sends nothing");
  assert.deepEqual(runRequest(workflow, BY_HAND, { audience: "the team" }, ""), { kind: "run", inputs: { audience: "the team" } }, "the inputs, and nothing about a goal");

  const dialog = src("../views/_workflow/RunWorkflowDialog.tsx");
  assert.ok(dialog.includes("runRequest(workflow, entry, values, payload)"), "what it sends is the model's");
  assert.ok(dialog.includes("api.runWorkflow(workflow.id, request.inputs)") && dialog.includes("api.testRunWorkflow(workflow.id, request.body)"));
  assert.ok(!dialog.includes("GoalPicker") && !dialog.includes("statement") && !dialog.includes("target"), "no goal to pick, capture or aim at");
  assert.ok(dialog.includes('navigate({ name: "run", id: made.run.id })'), "the run's page next");

  const route = parse(href({ name: "run", id: "01R1" }), { name: "inbox" });
  assert.deepEqual(route, { name: "run", id: "01R1" });
  assert.equal(section(route), "workflows", "the sidebar keeps Workflows lit on a run's page");
  const app = src("../App.tsx");
  assert.ok(app.includes('case "run":') && app.includes("<WorkflowRun key={route.id} id={route.id} />"), "the route mounts the run's page");
});

test("a workflow whose steps read the goal runs on a goal only: no Run…, no Turn on…, the card and the pane say so — and the node's refusal is said by name", () => {
  const needs = [{ step: "draft", kind: "needs_goal", text: { id: "problem-needs-goal", args: { step: "draft" } } }];
  const readsGoal = row({ workspace_problems: needs, starts: [...row().starts, { step: "nightly", event: "schedule", summary: { id: "step-summary-start-cron", args: { cron: "0 2 * * *" } } }] });
  const verbs = workflowVerbs(readsGoal);
  assert.equal(verbs.run, null);
  assert.equal(verbs.turnOn, null, "an event would start a run in the workspace too");
  assert.deepEqual(cardMenu(verbs).map((i) => i.id), ["open", "delete"]);
  assert.deepEqual(cardStatus(readsGoal), { tone: "quiet", icon: "goal", words: "runs on a goal", live: false });
  assert.equal(switchState(readsGoal).words, "Can't turn on: a step reads the goal it serves, and a run in the workspace has none");
  assert.match(PROBLEM_KIND_LABEL.needs_goal, /goal/, "the problem has words a designer can act on");
  const pane = src("../views/_workflow/WorkflowRunsPane.tsx");
  assert.ok(pane.includes("(row.workspace_problems ?? []).length > 0") && pane.includes('t("workflow-runs-pane-runs-on-goal-only")'), "the Runs pane says why it offers no Run…");

  // The card was drawn before the step was: the node refuses the start, by name, and the desktop says it and reads the row again.
  const refusal = runRefusal({ error: "the workflow cannot start", problems: needs }, "the workflow cannot start");
  assert.deepEqual(refusal, { words: "It runs on a goal only: draft reads the goal it serves, and a run in the workspace has none.", needsGoal: true, reread: true });
  const dialog = src("../views/_workflow/RunWorkflowDialog.tsx");
  assert.ok(dialog.includes("runRefusal(e instanceof ApiError ? e.body : undefined") && dialog.includes("if (refusal.reread)"), "the dialog says the refusal and asks for the row again");
});

test("two runs go at once: the card counts them, the Runs pane lists them live first, each numbered, each saying who started it", () => {
  const going = row({ runs: { live: 2, total: 3 } });
  assert.equal(cardStatus(going).words, "running 2 runs");
  assert.equal(cardStatus(going).live, true, "the dot pulses");
  const verbs = workflowVerbs(going);
  assert.deepEqual([verbs.stop?.live, verbs.restart?.live], [2, 2]);
  assert.ok(verbs.run, "a third may start beside them — runs of the workspace never queue");
  assert.deepEqual(cardMenu(verbs).map((i) => i.id), ["open", "run", "restart", "stop", "delete"]);

  const rows = runsPaneRows([
    summary("r3", "waiting", 3),
    summary("r2", "done", 2, { finished_at: 9, started_by: { by: "event", event: "message", detail: "Maya" } }),
    summary("r1", "running", 1, { started_by: { by: "event", event: "schedule" } }),
  ]);
  assert.deepEqual(rows.map((r) => [r.id, r.title, r.live]), [
    ["r3", "Nightly report #3", true],
    ["r1", "Nightly report #1", true],
    ["r2", "Nightly report #2", false],
  ]);
  assert.deepEqual(rows.map((r) => r.startedBy), ["by you", "by schedule", "by message from Maya"], "a run a start event began says which event");
  assert.deepEqual(rows.map((r) => [r.words.word, r.words.atWord]), [["waiting", "started"], ["running", "started"], ["done", "finished"]]);
  assert.deepEqual(rows[0].verbs, { stop: true, restart: true, open: true });
  assert.deepEqual(rows[2].verbs, { stop: false, restart: true, open: true }, "a finished run starts again with its inputs");

  // The pane follows the bus: a fact about a run of this workflow, and no other.
  assert.equal(movesRunsOf(fact("step_changed", "r3"), "01WF"), true);
  assert.equal(movesRunsOf({ workflow: "01OTHER", payload: { type: "run_finished", run: "r9" } }, "01WF"), false);
  assert.equal(movesRunsOf({ workflow: "01WF", payload: { type: "goal_status" } }, "01WF"), false);
});

test("a run ends while the pane is open: its row leaves the ones going, keeps its number, and offers no Stop; the card and the designer count again", () => {
  const before = runsPaneRows([summary("r3", "waiting", 3), summary("r2", "running", 2), summary("r1", "done", 1, { finished_at: 4 })]);
  assert.deepEqual(before.map((r) => [r.id, r.live, r.verbs.stop]), [["r3", true, true], ["r2", true, true], ["r1", false, false]]);

  // `run_finished` for r3: the pane reads again (`movesRunsOf`), and so do the library's rows and the designer's own.
  const ended = fact("run_finished", "r3", { outcome: "done" });
  assert.ok(movesRunsOf(ended, "01WF"));
  assert.deepEqual(libraryReads(ended.payload), { library: true, catalog: false });
  assert.ok(movesRunCount(ended, "01WF"), "the designer's menu stops offering a verb for a run that ended");
  assert.ok(!movesRunCount(fact("step_changed", "r2"), "01WF"), "a step inside a run moves no count");
  const designer = src("../views/WorkflowDesigner.tsx");
  assert.ok(designer.includes("if (movesRunCount(e, id)) reload();"), "the designer follows its runs whichever pane is open");

  const after = runsPaneRows([summary("r3", "done", 3, { finished_at: 12 }), summary("r2", "running", 2), summary("r1", "done", 1, { finished_at: 4 })]);
  assert.deepEqual(after.map((r) => [r.id, r.title, r.live, r.words.word, r.verbs.stop]), [
    ["r2", "Nightly report #2", true, "running", true],
    ["r3", "Nightly report #3", false, "done", false],
    ["r1", "Nightly report #1", false, "done", false],
  ]);
  assert.equal(cardStatus(row({ runs: { live: 1, total: 3 } })).words, "running one run");
  assert.equal(cardStatus(row({ runs: { live: 0, total: 3 } })).words, "ready to run");
});

test("the run's page: the header, what it owes, its steps read downward, the read-only canvas — and it follows its own run", () => {
  const run = {
    id: "r3",
    workflow,
    inputs: { audience: "the team" },
    queued_at: 3,
    started_at: 3,
    outcome: null,
    cancelled: null,
    steps: {
      begin: { state: { state: "done" }, visits: 1 },
      draft: { state: { state: "done" }, visits: 1, started_at: 3, finished_at: 63, work_item: "01J0000000000000000ITEM1", output: { summary: "Up 4%." } },
      review: { state: { state: "waiting" }, visits: 1, started_at: 63 },
    },
  };
  const ask = { gate_id: "g1", kind: "step", step: "review", home: { home: "run", run: "r3" } };
  const view = { run, summary: summary("r3", "waiting", 3, { started_by: { by: "event", event: "schedule" } }), holder: "you", needs_actions: [ask] };

  // The header: its title, its status, who it waits on, when it started, who started it, its verbs.
  const facts = runPageFacts(view);
  assert.deepEqual([facts.title, facts.words.word, facts.words.tone, facts.holder, facts.words.atWord, facts.words.at, facts.startedBy, facts.revision], ["Nightly report #3", "waiting", "warn", "your move", "started", 3, "by schedule", "rev 3"]);
  assert.deepEqual(facts.verbs, { stop: true, restart: true, open: true });
  assert.equal(facts.handsTo, null, "a run of the workspace is the page's own");

  // Progress: a start is no row; the live step stands open, with the verbs it admits.
  const rows = progressRows(run, { current: facts.live, steps: [] }, 123);
  assert.deepEqual(rows.map((r) => [r.id, r.state.state, r.holder, r.duration, r.actions]), [
    ["draft", "done", null, "1m", ["open"]],
    ["review", "waiting", "you", "1m", ["answer", "done"]],
    ["send", "pending", null, null, []],
  ]);
  assert.deepEqual(rows.map((r) => rowOpen(new Set(), r.id, r.current)), [false, true, false]);

  // Your move, Progress and Canvas are the page's three parts; the canvas takes no edit.
  const page = src("../views/WorkflowRun.tsx");
  assert.ok(page.includes("<YourMoveBand actions={needs_actions}"), "the page answers what the run owes, through its home");
  assert.ok(page.includes("<RunSteps") && page.includes("strip={{ current: facts.live, steps: [] }}"), "the page reads the run's steps as the goal page does");
  assert.ok(page.includes("run={run}") && page.includes("onChange={() => {}} readOnly"), "the frozen workflow, read-only, wearing the run");
  assert.ok(page.includes("runPageFacts(data)") && page.includes("movesRun(e, id, workflow)"), "what it says and what moves it are the model's");
  // What a restarted node resumed, failed or withdrew reaches a screen by no frame: each reads again when the bus comes back.
  // A read made through `useAsync` is one: the hook reads again for every read it holds.
  assert.ok(src("../views/_work/useAsync.ts").includes("useReloadOnReconnect(reload)"), "the shared read hook");
  assert.ok(page.includes("useAsync((s) => api.run(id, s)"), "the run's page");
  assert.ok(src("../views/_workflow/WorkflowRunsPane.tsx").includes("useReloadOnReconnect(moved);"), "the Runs pane, which holds its rows by hand");
  const designer = src("../views/WorkflowDesigner.tsx");
  assert.ok(designer.includes("useAsync((s) => api.workflow(id, s)") && designer.includes("api.workflowListeners(id, s)"), "the designer's row and its listeners");

  // It follows its own run: a step, a gate opened on it by a guard or an agent's question, a decision made in the Inbox.
  for (const type of ["step_changed", "gate_opened", "question_asked", "gate_decided", "run_finished"]) assert.ok(movesRun(fact(type, "r3"), "r3", "01WF"), type);
  assert.ok(!movesRun(fact("step_changed", "r2"), "r3", "01WF"), "its sibling's step redraws nothing here");
  // Its workflow deleted takes it along: the page reads again, finds nothing, and leaves for the library.
  assert.ok(movesRun({ payload: { type: "workflow_deleted", workflow: "01WF" } }, "r3", "01WF"));
  assert.ok(page.includes('useGonePlace(missing, { name: "run", id })'));

  // A goal's run opened by its id is its goal's to show.
  const goals = runPageFacts({ ...view, summary: { ...view.summary, scope: "goal", goal: "01GOAL" } });
  assert.deepEqual(goals.handsTo, { route: { name: "goal", id: "01GOAL" }, search: { tab: "workflow", run: "r3" } });
  assert.ok(page.includes('replace({ name: "goal", id: goal }, { tab: "workflow", run: id })'));
});

test("the review sent it back until the draft's visits ran out: the spent step reads as failed with its reason, its path is lit, the run went on", () => {
  // Review → Draft is the rework loop; Draft passes over its own failure (`on_fail: skip`).
  const looped = { ...workflow, steps: steps.map((s) => (s.id === "review" ? { ...s, then: [{ to: "send" }, { to: "draft" }] } : s)) };
  const run = {
    id: "r4",
    workflow: looped,
    queued_at: 20,
    started_at: 20,
    finished_at: 90,
    outcome: "done",
    cancelled: null,
    steps: {
      begin: { state: { state: "done" }, visits: 1, seq: 1 },
      draft: { state: { state: "failed" }, spent: true, visits: 3, seq: 14, started_at: 70, finished_at: 80, error: "entered 3 times; max_visits is 3" },
      review: { state: { state: "done" }, visits: 3, seq: 15 },
      send: { state: { state: "done" }, visits: 1, seq: 16, output: { message: "Sent." } },
    },
  };
  assert.equal(stepTone(run, "draft"), "danger");
  assert.equal(stepLabel(run, "draft"), "failed: entered 3 times; max_visits is 3");
  const tone = (from, to) => edgeTone(run, toGraph(looped).edges.find((e) => e.from === from && e.to === to));
  assert.equal(tone("draft", "review"), "taken", "passed over: the run went on along the step's own flow");
  const rows = progressRows(run, { current: [], steps: [] }, 100);
  assert.deepEqual(rows.map((r) => [r.id, r.state.state, r.error]), [
    ["draft", "failed", "entered 3 times; max_visits is 3"],
    ["review", "done", null],
    ["send", "done", null],
  ]);
  const facts = runPageFacts({ run, summary: summary("r4", "done", 4, { finished_at: 90 }), holder: "finished", needs_actions: [] });
  assert.deepEqual([facts.finished, facts.words.word, facts.live, facts.ended], [true, "done", [], "The run finished done."]);
  assert.deepEqual(facts.verbs, { stop: false, restart: true, open: true });
  // Had it failed the run instead (`on_fail: fail`), the line under the rows names the step and why.
  const failed = runPageFacts({ run: { ...run, outcome: "failed" }, summary: summary("r4", "failed", 4, { finished_at: 90 }), holder: "finished", needs_actions: [] });
  assert.equal(failed.ended, "The run failed at Draft — entered 3 times; max_visits is 3.");
});

test("the run's ask reaches the Inbox on its workflow's row, whose door is the run's page; its worker is named by its run", () => {
  const inboxRow = { key: "01WF", kind: "workflow", title: "Nightly report", needs_action: [{ id: "step:r3/review", kind: "step", home: { home: "run", run: "r3" } }], notices: [] };
  assert.deepEqual(doorOf(inboxRow), { route: { name: "run", id: "r3" }, search: null });
  assert.deepEqual(doorOf({ ...inboxRow, needs_action: [] }), { route: { name: "workflow", id: "01WF" }, search: null }, "with nothing asked, the designer");
  assert.deepEqual(attachedTo({ run: "01JRUN0000000R3ASK", kind: "run" }).route, { name: "run", id: "01JRUN0000000R3ASK" });
});

test("one run stopped, then every run: a press is taken once, and the confirm says a goal's run of it goes on", () => {
  // Stop on r3's row: taken; pressed again before the node answered: nothing more is sent.
  let flying = taken(new Set(), "r3");
  assert.ok(inFlight(flying, "r3"));
  assert.equal(taken(flying, "r3"), null, "Restart pressed twice would start two runs");
  assert.ok(taken(flying, "r2"), "its sibling's verbs are its own");
  flying = settled(flying, "r3");
  assert.ok(!inFlight(flying, "r3"));
  const verbsHook = src("../views/_workflow/useRunVerbs.ts");
  assert.ok(verbsHook.includes("taken(flying.current, run)") && verbsHook.includes("settled(flying.current, run)") && verbsHook.includes("api.stopRun(run)") && verbsHook.includes("api.restartRun(run)"));
  for (const screen of ["../views/_workflow/WorkflowRunsPane.tsx", "../views/WorkflowRun.tsx"]) {
    const text = src(screen);
    assert.ok(text.includes("useRunVerbs(") && !text.includes("api.stopRun(") && !text.includes("api.restartRun("), `${screen} stops and restarts through the one hook`);
  }

  assert.deepEqual(runRowVerbs(summary("r3", "cancelled", 3, { cause: { cause: "stopped", rationale: null } })), { stop: false, restart: true, open: true });
  assert.equal(runsPaneRows([summary("r3", "cancelled", 3, { finished_at: 30, cause: { cause: "stopped", rationale: null } })])[0].words.word, "stopped");
  assert.equal(stopEveryWords(1), "The run of this workflow that is going is cancelled and its sessions end. A goal's run of it is its goal's, and goes on.");
  assert.match(restartEveryWords(2), /^The 2 runs of this workflow that are going are cancelled and a new run starts at once in place of each/);
  const card = src("../views/_workflow/WorkflowCard.tsx");
  assert.ok(card.includes("api.stopWorkflow(row.workflow.id)") && card.includes("api.restartWorkflow(row.workflow.id)"));
});

test("a workflow that listened for a year kept every run: the pane draws a page of them, the ones going first, and says what it left out", () => {
  const year = Array.from({ length: 365 }, (_, i) => summary(`r${365 - i}`, i === 364 ? "waiting" : "done", 365 - i, { started_by: { by: "event", event: "schedule" } }));
  const all = runsPaneRows(year);
  assert.equal(all.length, 365);
  const first = paneWindow(all);
  assert.deepEqual([first.rows.length, first.hidden, first.more], [RUNS_SHOWN, 315, RUNS_SHOWN]);
  assert.deepEqual(first.rows.slice(0, 2).map((r) => r.title), ["Nightly report #1", "Nightly report #365"], "the one still going, then the newest");
  const older = paneWindow(all, RUNS_SHOWN * 2);
  assert.deepEqual([older.rows.length, older.hidden], [100, 265]);
  const pane = src("../views/_workflow/WorkflowRunsPane.tsx");
  assert.ok(pane.includes("paneWindow(all, shown)") && pane.includes("setShown((n) => n + RUNS_SHOWN)"));
});

test("retiring the workflow retires its runs going first; archived, its runs stay as history and none restarts; deleted, they go", () => {
  const preview = { agents: 1, harnesses: 0, run: null, runs: [{ id: "r1", status: "running", live_steps: 1 }], history: 3, refusal: null, designs: 0, used_by: [], projects_born: [], projects_attached: [] };
  assert.equal(runsLine(preview), "Its run in the workspace is cancelled: 1 step is live.");
  assert.equal(historyLine(preview, { thing: "archive", projects: "keep", tree: false }), "Its 3 runs in the workspace stay as its history.");
  const sections = retireSections("workflow", preview, { thing: "delete", projects: "keep", tree: false }, { harnesses: 0, shells: 0, agents: 1 });
  assert.deepEqual(sections.map((s) => s.id), ["run", "stops", "history"]);
  assert.equal(sections[2].lines[0], "Its 3 runs in the workspace go with it.");

  // Archived: the card says so, nothing starts, and a run of its history offers no Restart the node would refuse.
  const archived = row({ workflow: { ...workflow, archived: { at: 50 } }, runs: { live: 0, total: 3 } });
  assert.equal(cardStatus(archived).words, "archived");
  assert.deepEqual(cardMenu(workflowVerbs(archived)).map((i) => i.id), ["open", "delete"]);
  const history = runsPaneRows([summary("r1", "cancelled", 1, { finished_at: 50, cause: { cause: "retired" } })], { archived: true });
  assert.deepEqual([history[0].words.word, history[0].verbs], ["retired", { stop: false, restart: false, open: true }]);
  assert.ok(src("../views/_workflow/WorkflowRunsPane.tsx").includes("{ archived: Boolean(row.workflow.archived) }"));
});

test("what a run may spend is its own: Settings › Automation › Budgets draws budget.*, off the Goals panel", () => {
  const settings = src("../views/Settings.tsx");
  assert.ok(settings.includes('<RegistryPanel group="budget" />'), "the registry panel over budget.*");
  const rail = src("../views/_settings/settingsLink.mjs");
  const automation = rail.slice(rail.indexOf('group("automation"'), rail.indexOf('group("performance"'));
  assert.ok(automation.includes('panel("budgets",') && settings.includes('panel.id === "budgets" &&'), "a panel of its own under Automation");
  assert.ok(settings.includes('<RegistryPanel group="events" />') && automation.includes('panel("events",'), "the events runtime's own settings sit beside it, under Automation › Events");
});
