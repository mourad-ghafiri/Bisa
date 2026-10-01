/**
 * Events and gateways as a person meets them (03-workflows §Start events ·
 * §Boundary events · §Gateways · §Listening): a workflow designed from *New
 * workflow* — its start moved onto a schedule, a parallel gateway, a
 * review with a reminder beside it and a timeout that diverts it — turned
 * On from its designer, a run *by schedule* on the Runs pane with the
 * timeout's path lit, and a goal whose design begins on events listening,
 * then paused by a failed run, on its page. Every step reads the same
 * models the screens draw from; no DOM. Run with
 * `node --test --import ./desktop/src/i18n/preload.mjs desktop/src/scenarios/events.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { BOUNDARY_EVENTS, FAMILIES, FAMILY_LABEL, START_EVENTS, START_STEP_ID, blankStep, blankWorkflow, branchesOf, familyOf, mayCarryBoundaries } from "../views/_workflow/stepKinds.mjs";
import { addStep, connect, failChoice, failTargets, freshBranch, replaceStep, toGraph } from "../views/_workflow/workflowGraph.mjs";
import { actsFor, addBoundary, boundaryOf, chipOf, consequence, removeBoundaryOn, renameBoundaryOn, replaceBoundary, setAct, setEvent } from "../views/_workflow/forms/boundaryModel.mjs";
import { addField, renameField, setFieldValue } from "../views/_workflow/forms/exactFieldsModel.mjs";
import { PAYLOAD_FIELDS, blankStartOn, eventOf, eventPhrase, eventStarts, guardWords, listeningNeeds, manualEntry, mappingRows, mappingSuggestions, samplePayload, setCadence, setGuard, setMapping, setStartEvent, strayMappings, validSignalName } from "../views/_workflow/forms/startForm.mjs";
import { heldOf, heldWords, hostOf, movesSignalsOf, signalsQuery } from "../views/_workflow/heldSignalsModel.mjs";
import { mintedBy, shownFor, shownWith } from "../views/_workflow/hookSecretsModel.mjs";
import { estimateOf, sideOnly } from "../views/_workflow/workflowLayout.mjs";
import { thumbnail } from "../views/_workflow/thumbnailModel.mjs";
import { againBody, neededInputs, nextWords, readBudget, switchState, turnOnBody, turnOnBlockers, goalListening, movesListeningOf } from "../views/_workflow/listeningModel.mjs";
import { workflowVerbs } from "../views/_workflow/workflowVerbs.mjs";
import { BY_HAND, askedInputs, firstEntry, runEntries, runRefusal, runRequest, sampleText } from "../views/_workflow/runDialogModel.mjs";
import { cardMenu, onMark } from "../views/_workflow/workflowCardModel.mjs";
import { rowStatuses } from "../views/_workflow/libraryModel.mjs";
import { runsPaneRows } from "../views/_workflow/workflowRunsModel.mjs";
import { edgeTone, firedBoundaries, progress, stepLabel, stepTone } from "../views/_workflow/runView.mjs";
import { progressRows } from "../views/_goal/progressModel.mjs";
import { listenVerb, runVerbs } from "../views/_goal/runControl.mjs";
import { pageFacts } from "../views/_goal/goalPageModel.mjs";
import { listeningChip } from "../views/_goals/goalCardModel.mjs";

const src = (name) => readFileSync(new URL(name, import.meta.url), "utf8");
const step = (wf, id) => wf.steps.find((s) => s.id === id);
const must = (r) => {
  assert.equal(r.ok, true, r.reason);
  return r.wf;
};
const DAY = 86400;

/** The weekly report, drawn the way the designer draws it: one edit at a time on the graph. */
function weeklyReport() {
  // *New workflow* opens on a start by hand — moved onto a Monday-morning schedule.
  let wf = blankWorkflow("Weekly report");
  const begin = setStartEvent(step(wf, START_STEP_ID), "schedule");
  wf = replaceStep(wf, START_STEP_ID, { ...begin, on: setCadence(begin.on, "cron") });
  wf = { ...wf, inputs: [{ name: "audience", label: "Audience", kind: "text", required: true }] };
  // A parallel gateway fans out to two agents; their paths meet at the review.
  for (const [kind, id] of [["parallel", "fan"], ["agent", "draft"], ["agent", "numbers"], ["human", "review"], ["notify", "escalate"], ["end", "done"]]) {
    wf = addStep(wf, kind, id).wf;
  }
  // The review wears a reminder beside it and a timeout that diverts it.
  let review = addBoundary(addBoundary(step(wf, "review"), "every"), "after");
  review = replaceBoundary(review, "reminder", { ...boundaryOf(review, "reminder"), template: "The weekly report waits on your review." });
  review = replaceBoundary(review, "timeout", { ...boundaryOf(review, "timeout"), on: { event: "after", secs: 2 * DAY } });
  wf = replaceStep(wf, "review", review);
  for (const [from, to, branch] of [
    [START_STEP_ID, "fan"],
    ["fan", "draft"],
    ["fan", "numbers"],
    ["draft", "review"],
    ["numbers", "review"],
    ["review", "done"],
    ["review", "escalate", "timeout"],
    ["escalate", "done"],
  ]) {
    wf = must(connect(wf, from, to, branch ?? null));
  }
  return wf;
}

const stored = { ...weeklyReport(), id: "01WFWEEKLY", origin: { origin: "workspace" }, archived: null, revision: 3 };
const scheduleStart = { step: START_STEP_ID, event: "schedule", summary: { id: "step-summary-start-cron", args: { cron: "0 9 * * 1" } } };
const row = (over = {}) => ({
  workflow: stored,
  problems: [],
  workspace_problems: [],
  used_by: [],
  runs: { live: 0, total: 0 },
  listening: null,
  starts: [scheduleStart],
  event_only: true,
  listening_needs: ["audience"],
  ...over,
});
/** Monday 28 September 2026, 09:00 UTC: when the schedule next comes due. */
const MONDAY_9 = 1790586000;
const listener = { listener: `workspace:${stored.id}/${START_STEP_ID}`, host: `workspace:${stored.id}`, step: START_STEP_ID, event: "schedule", summary: scheduleStart.summary, next_due: MONDAY_9, backlog: 0, live_runs: 0 };

test("designed from New workflow: a schedule start, a parallel gateway, a reminder beside the review and a timeout that diverts it", () => {
  const wf = weeklyReport();
  // The palette's four groups, in the order it draws them.
  assert.deepEqual(FAMILIES.map((f) => FAMILY_LABEL[f]), ["Events", "Gateways", "Loops", "Tasks"]);
  assert.deepEqual([familyOf("start"), familyOf("parallel"), familyOf("human")], ["event", "gateway", "task"]);

  // The start, moved off by hand: what its card's second line says; its ways in, read the core's way.
  const begin = step(wf, START_STEP_ID);
  assert.deepEqual(begin.on, { event: "schedule", cron: "0 9 * * 1", tz: null });
  assert.equal(eventPhrase(begin.on), "on the schedule 0 9 * * 1");
  assert.equal(manualEntry(wf), null, "no start by hand is left: only its schedule begins it");
  assert.deepEqual(eventStarts(wf).map((s) => s.id), [START_STEP_ID]);
  assert.deepEqual(listeningNeeds(wf), ["audience"], "a required input its schedule does not supply: turning On asks it");
  assert.deepEqual(samplePayload(begin.on, 1000), { at: 1000 }, "a test run's payload: the moment it fired");

  // The review's boundaries: the reminder acts beside it; the timeout diverts, so its name is a label a flow carries.
  const review = step(wf, "review");
  assert.deepEqual(review.boundaries.map((b) => [b.name, b.on.event, b.act]), [["reminder", "every", "notify"], ["timeout", "after", "divert"]]);
  assert.deepEqual(branchesOf(review), ["timeout"], "a reminder never diverts");
  assert.deepEqual(review.boundaries.map((b) => chipOf(b).words), ["every 1d · up to 3", "after 2d"]);
  assert.equal(consequence(review, boundaryOf(review, "timeout")), "Diverts to escalate.");
  assert.equal(consequence(review, boundaryOf(review, "reminder")), "Posts beside the live step; the step goes on.");
  assert.match(connect(wf, "review", "escalate", "reminder").reason, /^only a branching step/, "a reminder carries no path");
  assert.equal(connect(wf, "fan", START_STEP_ID).ok, false, "nothing flows into a start");

  // The graph: the timeout's path is a boundary edge; the gateway's two flows are plain.
  const { edges } = toGraph(wf);
  assert.deepEqual(
    edges.filter((e) => e.from === "review").map((e) => [e.to, e.branch, e.kind]),
    [["done", null, "then"], ["escalate", "timeout", "boundary"]],
  );
  assert.deepEqual(edges.filter((e) => e.from === "fan").map((e) => [e.to, e.kind]), [["draft", "then"], ["numbers", "then"]]);
  // The layout makes room for the chips and stands the timeout's step to the right of its rank.
  assert.ok(estimateOf(review).height > estimateOf(step(wf, "draft")).height, "a card with chips is taller");
  assert.deepEqual([...sideOnly(edges)], ["escalate"]);
  // The library's thumbnail draws the start as an event, the gateway as one, and the timeout's path apart.
  const thumb = thumbnail(wf);
  assert.deepEqual([START_STEP_ID, "fan", "review"].map((id) => thumb.nodes.find((n) => n.id === id)?.family), ["event", "gateway", "task"]);
  assert.equal(thumb.edges.filter((e) => e.kind === "boundary").length, 1);
});

test("turned On from its designer: the switch asks what its schedule does not supply, then says when it next comes due", () => {
  // Off: the designer's header switch, the card's verbs, no mark.
  const off = row();
  assert.deepEqual(switchState(off), { state: "off", words: "Off", tone: "quiet", toggle: "on", again: false });
  const offVerbs = workflowVerbs(off);
  assert.equal(offVerbs.run?.label, "Test run…", "only events begin it: a run by hand is a test");
  assert.deepEqual(cardMenu(offVerbs).map((i) => i.id), ["open", "run", "turn_on", "delete"]);
  assert.equal(onMark(off), null);

  // Turn on…: the one input its events leave unset, and a per-run ceiling.
  assert.deepEqual(neededInputs(off).map((i) => i.name), ["audience"]);
  assert.deepEqual(turnOnBody({ audience: "the team" }, { max_usd_cents: 500, max_tokens: "", max_wall_clock_secs: null }), { inputs: { audience: "the team" }, budget: { max_usd_cents: 500 } });
  const dialog = src("../views/_workflow/TurnOnDialog.tsx");
  assert.ok(dialog.includes("api.turnOnWorkflow(row.workflow.id, turnOnBody("), "the dialog sends the model's body");
  assert.ok(dialog.includes("<HookSecretNote"), "a public hook's secret is shown once, here");

  // On: the node's words for what it hears and the soonest it comes due; the card wears its mark.
  const on = row({ listening: { inputs: { audience: "the team" }, budget: { max_usd_cents: 500 }, since: 1790000000 } });
  const state = switchState(on, [listener]);
  assert.equal(state.state, "on");
  assert.equal(state.words, `On — begins on the schedule \`0 9 * * 1\` · next ${nextWords(MONDAY_9)}`);
  assert.deepEqual(cardMenu(workflowVerbs(on)).map((i) => i.id), ["open", "run", "turn_off", "delete"]);
  assert.deepEqual(onMark(on), { words: "On", tone: "ok" });
  assert.ok(rowStatuses(on).includes("on"), "the library's On filter finds it");

  // The designer draws the switch and re-reads when the node says its listening moved; a goal's design has none.
  const designer = src("../views/WorkflowDesigner.tsx");
  assert.ok(designer.includes("<ListeningSwitch row={data}") && designer.includes("movesListeningOf(e, `workspace:${id}`)"));
  assert.equal(movesListeningOf({ payload: { type: "listening_changed", host: `workspace:${stored.id}`, on: true } }, `workspace:${stored.id}`), true);
  assert.equal(movesListeningOf({ payload: { type: "listening_changed", host: "workspace:01OTHER", on: true } }, `workspace:${stored.id}`), false);
  const goalsOwn = row({ workflow: { ...stored, origin: { origin: "goal", goal: "01GOAL" } } });
  assert.equal(workflowVerbs(goalsOwn).turnOn, null, "its goal listens instead");
  assert.ok(src("../views/_workflow/ListeningSwitch.tsx").includes('row.workflow.origin.origin === "goal"'));

  // Run… offers the test alone — only its schedule begins it — as if the schedule had fired.
  assert.deepEqual(runEntries(stored).map((e) => [e.id, e.test, e.label, e.hint]), [[START_STEP_ID, true, "Test: as if “Start” happened", "on the schedule 0 9 * * 1"]]);
  assert.equal(firstEntry(stored), START_STEP_ID);
  assert.deepEqual(askedInputs(stored, START_STEP_ID), { asked: ["audience"], mapped: 0 }, "its schedule fills no input");
  assert.equal(sampleText(stored, START_STEP_ID, 1000), JSON.stringify({ at: 1000 }, null, 2));
  assert.deepEqual(runRequest(stored, START_STEP_ID, { audience: "the team" }, '{ "at": 1000 }'), { kind: "test", body: { inputs: { audience: "the team" }, start: START_STEP_ID, event: { at: 1000 } } });
  assert.deepEqual(runRequest(stored, BY_HAND, { audience: "the team" }, ""), { kind: "refused", errors: {}, payloadError: null }, "by hand is no way into it");
  const run = src("../views/_workflow/RunWorkflowDialog.tsx");
  assert.ok(run.includes("api.testRunWorkflow(workflow.id, request.body)") && run.includes("sampleText(workflow, next)"));
});

test("a run by schedule on the Runs pane: the gateway took both paths, the timeout diverted the review, its path is lit", () => {
  const rows = runsPaneRows([
    { id: "r2", status: "running", number: 2, scope: "workspace", workflow: stored.id, workflow_name: stored.name, revision: 3, queued_at: 2, started_by: { by: "event", event: "schedule" } },
    { id: "r1", status: "done", number: 1, scope: "workspace", workflow: stored.id, workflow_name: stored.name, revision: 3, queued_at: 1, started_by: { by: "test", event: "schedule" } },
  ]);
  assert.deepEqual(rows.map((r) => [r.title, r.startedBy]), [["Weekly report #2", "by schedule"], ["Weekly report #1", "test run"]]);

  const run = {
    id: "r2",
    workflow: stored,
    started_at: 10,
    outcome: null,
    cancelled: null,
    start: START_STEP_ID,
    steps: {
      [START_STEP_ID]: { state: { state: "done" }, visits: 1 },
      fan: { state: { state: "done" }, visits: 1 },
      draft: { state: { state: "done" }, visits: 1 },
      numbers: { state: { state: "done" }, visits: 1 },
      review: { state: { state: "diverted", by: "timeout" }, visits: 1, fired: { reminder: { count: 2, seq: 7, at: 100 }, timeout: { count: 1, seq: 8, at: 200 } } },
      escalate: { state: { state: "running" }, visits: 1 },
    },
  };
  assert.equal(stepLabel(run, "review"), "diverted → timeout");
  assert.equal(stepTone(run, "review"), "warn", "a detour, not a failure");
  assert.deepEqual([...firedBoundaries(run, "review")].sort(), ["reminder", "timeout"], "both chips are lit");
  const tone = (from, to) => edgeTone(run, toGraph(stored).edges.find((e) => e.from === from && e.to === to));
  assert.deepEqual([tone("fan", "draft"), tone("fan", "numbers")], ["taken", "taken"], "a parallel gateway takes every flow");
  assert.equal(tone("review", "escalate"), "taken", "the timeout's path");
  assert.equal(tone("review", "done"), "skipped", "a diverted step takes nothing else");
  // Its progress counts work: a start and a gateway are none; the diverted review reached its end.
  assert.equal(progress(run), 3 / 5);
  assert.deepEqual(progressRows(run, { current: ["escalate"] }).map((r) => r.id), ["draft", "numbers", "review", "escalate", "done"]);
});

test("a goal whose design begins on events listens on its page, then a failed run pauses it", () => {
  const goal = { id: "01GOALWEEKLY", mode: "manual", closed: null, workflow: stored.id, run: null, listening: null };
  const guidance = { mode: "manual", design_enabled: true, phase: "manual", design: null, open_questions: [] };
  const facts = (g) => pageFacts({ goal: g, run: null, pendingGates: [], startable: stored });
  assert.deepEqual([facts(goal).listens, facts(goal).manualEntry], [true, null]);
  const verbs = (g) => runVerbs({ goal: g, run: null, runs: [], guidance, proposed: false, startable: true, listens: true, manualEntry: facts(g).manualEntry });

  // Not listening yet: the page's start arms it.
  assert.deepEqual(verbs(goal).start, { label: "Start listening…", queues: false, adopt: false, listen: true });
  assert.equal(goalListening(goal.listening), null);
  assert.equal(listeningChip(goal), null);

  // Listening: the header's line and its verb; the Goals list's chip; nothing to run by hand.
  const listening = { ...goal, listening: { inputs: { audience: "the team" }, since: 1790000000 } };
  const goalListener = { ...listener, listener: `goal:${goal.id}/${START_STEP_ID}`, host: `goal:${goal.id}` };
  assert.deepEqual(goalListening(listening.listening, [goalListener]), {
    paused: false,
    words: `Listening — begins on the schedule \`0 9 * * 1\` · next ${nextWords(MONDAY_9)}`,
    tone: "accent",
  });
  assert.deepEqual(listenVerb(listening), { id: "stop", label: "Stop listening" });
  assert.equal(verbs(listening).start, null, "only events begin it");
  assert.deepEqual(listeningChip(listening), { words: "listening", tone: "accent", paused: false });

  // A run it started failed: paused, with Listen again — the inputs it listened with go back.
  const paused = { ...listening, listening: { ...listening.listening, paused: { reason: { reason: "run_failed", run: "01RFAILED" }, at: 1790600000 } } };
  assert.deepEqual(goalListening(paused.listening, [goalListener]), { paused: true, words: "Paused: a run it started failed", tone: "warn" });
  assert.deepEqual(listenVerb(paused), { id: "again", label: "Listen again" });
  assert.deepEqual(listeningChip(paused), { words: "paused", tone: "warn", paused: true });
  const header = src("../views/_goal/GoalHeader.tsx");
  assert.ok(header.includes("api.listenGoal(goal.id, { inputs: listening?.inputs ?? {} })") && header.includes("api.stopListeningGoal(goal.id)"));
  assert.ok(src("../views/GoalDetail.tsx").includes('"listening_changed"'), "the page re-reads when its listening moves");

  // The goal's start: listening asks only what its events leave unset, shows a minted secret once, and Run now begins at the start by hand.
  const dialogs = src("../views/_goal/RunVerbDialogs.tsx");
  assert.ok(dialogs.includes("startInputs(startable, listen)") && dialogs.includes("listen={listen}") && dialogs.includes("at={verbs.start?.at ?? null}"));
  const start = src("../views/_workflow/StartRunDialog.tsx");
  assert.ok(start.includes("api.startGoalRunAt(goal, { inputs: body, start: at })"), "Run now names the start it begins at");
  assert.ok(start.includes("<HookSecretNote") && start.includes("if (did.minted.length > 0) setSecrets(did.minted);"), "the dialog stays on a secret until the person has it");
  // A goal that listens has a start by hand to run now; the weekly report, begun by its schedule alone, has none.
  const withHand = { ...stored, steps: [{ id: "by-hand", name: "By hand", kind: "start", on: { event: "manual" }, then: [{ to: "fan" }] }, ...stored.steps] };
  assert.equal(pageFacts({ goal: listening, run: null, pendingGates: [], startable: withHand }).manualEntry, "by-hand");
  assert.deepEqual(
    runVerbs({ goal: listening, run: null, runs: [], guidance, proposed: false, startable: true, listens: true, manualEntry: "by-hand" }).start,
    { label: "Run now…", queues: false, adopt: false, at: "by-hand" },
  );
});

test("the start's form, event by event: each begins with its own fields, maps what its occurrence carries, and guards its runs", () => {
  // Ten ways in, in the words the form's select draws.
  assert.deepEqual(START_EVENTS.map((e) => e.label), [
    "By hand",
    "On a schedule",
    "When called",
    "When a message arrives",
    "When a signal is raised",
    "When a project changes",
    "When a run finishes",
    "When the platform says",
    "When an outside platform lists something new",
    "When a check starts failing",
  ]);
  // Picking an event writes a fresh `on` of it — and no blank reference: what is not chosen yet is `null`.
  let start = blankStep("start", "ticket");
  assert.equal(eventOf(start), "manual");
  for (const { event } of START_EVENTS) {
    start = setStartEvent(start, event);
    assert.equal(eventOf(start), event);
    assert.deepEqual(start.on, blankStartOn(event), event);
    for (const [field, value] of Object.entries(start.on)) if (["project", "connector", "operation", "account"].includes(field)) assert.equal(value, null, `${event}.${field} is a choice not made yet`);
    assert.notEqual(eventPhrase(start.on), "", `${event} has a phrase for its card`);
  }
  // A hook: the inputs it fills from the body, the rest left to what it listens with.
  const inputs = [
    { name: "ticket", label: "Ticket", kind: "text", required: true },
    { name: "customer", label: "Customer", kind: "text", required: false, default: "anon" },
  ];
  let hook = setStartEvent(blankStep("start", "ticket"), "hook");
  hook = setMapping(hook, "ticket", "{event.payload.body}");
  assert.deepEqual(mappingRows(hook, inputs), [
    { input: "ticket", label: "Ticket", required: true, template: "{event.payload.body}" },
    { input: "customer", label: "Customer", required: false, template: null },
  ]);
  assert.deepEqual(mappingSuggestions("hook"), [], "a hook's body is the caller's own shape: nothing to pick from");
  assert.deepEqual(mappingSuggestions("check"), PAYLOAD_FIELDS.check.map((f) => `{event.payload.${f}}`));
  // An input the workflow no longer declares is shown to be removed, never silently dropped.
  assert.deepEqual(strayMappings(setMapping(hook, "gone", "{event.payload.x}"), inputs), ["gone"]);
  assert.equal("inputs" in setMapping(hook, "ticket", "  "), false, "a blank template unmaps; an empty mapping is none");
  // Its guard, in words: four at once, and a debounce.
  hook = setGuard(hook, { overlap: "parallel", max: 4, debounce: 300 });
  assert.deepEqual(hook.guard, { debounce_secs: 300, overlap: { parallel: 4 } });
  assert.equal(guardWords(hook), "Up to 4 runs at once; the rest wait. One within 5m of the last run it started is dropped.");
  // Moved back to a start by hand, it maps nothing and guards nothing.
  const byHand = setStartEvent(hook, "manual");
  assert.deepEqual([byHand.on, "inputs" in byHand, "guard" in byHand], [{ event: "manual" }, false, false]);
});

test("what the designer refuses of an event: a flow or a failure led into a start, a signal that is no dotted words, a field that would take another's place", () => {
  const wf = weeklyReport();
  // Nothing flows into a start — by a flow drawn, or by a failure routed.
  assert.equal(connect(wf, "draft", START_STEP_ID).ok, false);
  assert.ok(!failTargets(wf.steps, "draft").some((s) => s.kind === "start"), "a start is no step a failure may be routed to");
  assert.deepEqual(failChoice(wf.steps, step(wf, "draft"), "then"), { on_fail: "then", step: "fan" });
  assert.equal(failChoice(blankWorkflow().steps, blankWorkflow().steps[0], "then"), null, "a workflow of one start has nowhere to route a failure");
  // A signal's name: dotted lowercase words — a start's, a wait's, a boundary's, an emit's.
  assert.ok(validSignalName("report.ready"));
  for (const bad of ["Report Ready", "report..ready", ""]) assert.ok(!validSignalName(bad), bad);
  // Its exact fields: a path is renamed where it stands, never onto another row.
  let fields = addField(addField({}));
  fields = setFieldValue(renameField(fields, "field", "env").fields, "env", "prod");
  assert.deepEqual(fields, { env: "prod", "field-2": "" });
  assert.deepEqual(renameField(fields, "field-2", "env"), { ok: false, reason: "Another field already matches `env`." });
  assert.deepEqual(Object.keys(renameField(fields, "env", "environment").fields), ["environment", "field-2"], "the row keeps its place");
  // A boundary sits only on a step whose work can be stopped, and a reminder never diverts.
  assert.deepEqual(wf.steps.filter(mayCarryBoundaries).map((s) => s.id), ["draft", "numbers", "review"]);
  assert.deepEqual(BOUNDARY_EVENTS.map((b) => [b.event, actsFor(b.event)]), [
    ["after", ["divert", "notify", "emit"]],
    ["every", ["notify", "emit"]],
    ["message", ["divert", "notify", "emit"]],
    ["signal", ["divert", "notify", "emit"]],
  ]);
  const review = step(wf, "review");
  assert.equal(setAct(review, "reminder", "divert"), review, "refused: the step keeps its act");
  // A timeout that becomes a reminder posts instead, and its path goes; renamed, a divert carries its path; removed, the path goes with it.
  assert.deepEqual(setEvent(review, "timeout", "every").then, [{ to: "done" }]);
  assert.deepEqual(renameBoundaryOn(review, "timeout", "late").step.then, [{ to: "done" }, { to: "escalate", branch: "late" }]);
  assert.equal(renameBoundaryOn(review, "timeout", "reminder").ok, false, "a name the step already uses");
  assert.deepEqual(removeBoundaryOn(review, "timeout").then, [{ to: "done" }]);
  // A gateway's new branch takes a name its step has for nothing else.
  const route = { id: "route", name: "Route", kind: "decide", pick: "every", rules: [{ when: { condition: "all", of: [] }, branch: "branch-1" }], otherwise: "otherwise", then: [] };
  assert.equal(freshBranch(route), "branch-2");
});

test("what turning On asks: the inputs no event supplies and a ceiling for each run — and what it refuses", () => {
  const off = row();
  assert.deepEqual(neededInputs(off).map((i) => [i.name, i.required]), [["audience", true]]);
  // A ceiling typed as a person types it; blank is none, and the workspace's default applies.
  assert.deepEqual(readBudget({ dollars: "5", tokens: "", minutes: "30" }), { budget: { max_usd_cents: 500, max_tokens: null, max_wall_clock_secs: 1800 }, errors: {} });
  assert.deepEqual(turnOnBody({ audience: "the team" }, readBudget({ dollars: "", tokens: "", minutes: "" }).budget), { inputs: { audience: "the team" } });
  // What is no ceiling is said, and nothing is sent: read as none, every run would go without one.
  assert.deepEqual(Object.keys(readBudget({ dollars: "a lot", tokens: "", minutes: "-1" }).errors), ["dollars", "minutes"]);
  const dialog = src("../views/_workflow/TurnOnDialog.tsx");
  assert.ok(dialog.includes("const ceiling = readBudget(budget);") && dialog.includes("Object.keys(ceiling.errors).length > 0) return;"));
  // Refused by the node all the same — the row was older than the workflow: said by name, and the row is read again.
  assert.ok(dialog.includes("runRefusal(e instanceof ApiError ? e.body : undefined"), "a turn-on refused says why as a start refused does");
  assert.equal(runRefusal({ error: "refused", problems: [{ step: "draft", kind: "needs_goal", text: { id: "problem-needs-goal" } }] }, "refused").words, "It runs on a goal only: draft reads the goal it serves, and a run in the workspace has none.");
  // Refused before the dialog: problems, put away, a goal's own design, a step that reads its goal.
  assert.deepEqual(turnOnBlockers(row({ problems: [{ kind: "bad_timer" }] })), ["1 problem"]);
  assert.deepEqual(turnOnBlockers(row({ workflow: { ...stored, archived: { at: 1 } } })), ["it is archived"]);
  assert.deepEqual(turnOnBlockers(row({ workspace_problems: [{ step: "draft", kind: "needs_goal" }] })), ["a step reads the goal it serves, and a run in the workspace has none"]);
  assert.equal(switchState(row({ problems: [{ kind: "bad_timer" }, { kind: "bad_cron" }] })).words, "Can't turn on: 2 problems");
  // Paused by a spent budget: heard again with what it listened with.
  const paused = row({ listening: { inputs: { audience: "the team" }, budget: { max_usd_cents: 500 }, since: 1, paused: { reason: { reason: "budget_spent" }, at: 9 } } });
  assert.deepEqual([switchState(paused).words, switchState(paused).again], ["Paused: its budget is spent", true]);
  assert.deepEqual(againBody(paused.listening), { inputs: { audience: "the team" }, budget: { max_usd_cents: 500 } });
  // The header's switch follows what was just drawn: the designer reads its row again once its own save lands.
  assert.ok(src("../views/WorkflowDesigner.tsx").includes("if (!rowBehind(data, session)) return;"));
});

test("a public hook's secret is shown once: where it was minted, under its own hook, and never kept", () => {
  // Shapes only: these are not secrets, and nothing accepts them.
  const ticket = { step: "ticket", path: `/hooks/workspace:${stored.id}/ticket`, secret: "aa".repeat(32) };
  const order = { step: "order", path: `/hooks/workspace:${stored.id}/order`, secret: "bb".repeat(32) };
  // Turning On answers the secrets it minted: the dialog stays on them until the person has them, and opens empty the next time.
  assert.deepEqual(mintedBy({ workflow: row(), listeners: [], secrets: [ticket, order] }), [ticket, order]);
  const turnOn = src("../views/_workflow/TurnOnDialog.tsx");
  assert.ok(turnOn.includes("if (on.secrets.length > 0) setSecrets(on.secrets);") && turnOn.includes("setSecrets([]);"));
  // An adoption decided from a card, a capture, listening again: handed to the one dialog at the app's root.
  let shown = shownWith([], mintedBy({ secrets: [ticket] }));
  shown = shownWith(shown, mintedBy({ secrets: [order] }));
  assert.deepEqual(shown.map((s) => s.step), ["ticket", "order"]);
  // Rotated: the old one opens nothing any more, so the newest is shown alone.
  const rotated = { ...ticket, secret: "cc".repeat(32) };
  assert.deepEqual(shownWith(shown, [rotated]).map((s) => s.secret), [order.secret, rotated.secret]);
  // In the start's form a rotated secret is its own hook's, and no other step's form wears it.
  assert.equal(shownFor(rotated, "ticket"), rotated);
  assert.equal(shownFor(rotated, "order"), null);
  // Never kept: memory only, forgotten when dismissed; no read route gives it back, only whether one is minted.
  const store = src("../views/_workflow/hookSecretsStore.ts");
  assert.ok(!/localStorage|sessionStorage|writePref|viewMemory/.test(store), "a secret is never written to storage");
  assert.ok(store.includes("if (shown.length > 0) set(NONE);"), "dismissing forgets it");
  const listener = { ...ticket, secret: undefined, public_hook: { path: ticket.path, has_secret: true } };
  assert.equal("secret" in listener.public_hook, false, "a listener says a secret is minted, never what it is");
  for (const file of ["../views/_workflow/ListeningSwitch.tsx", "../views/_goal/GoalHeader.tsx"]) assert.ok(src(file).includes("showHookSecrets(mintedBy("), `${file}: what listening again mints is shown`);
});

test("a call from outside the content screen would not pass is held for the person, who lets it through", () => {
  const host = hostOf({ workflow: stored.id });
  assert.equal(host, `workspace:${stored.id}`);
  assert.deepEqual(signalsQuery(host), { host, limit: 200 });
  const signal = (id, state, over = {}) => ({ id, listener: `${host}/ticket`, source: "hook", at: 100, state, scope: { scope: "workspace" }, ...over });
  // Written down first, then read: held, its reason on it.
  const why = "held by the content screen — it asks the agent to print a token; let it through or leave it";
  let signals = [signal("s2", "held", { at: 200, note: why }), signal("s1", "done", { at: 100 })];
  assert.ok(movesSignalsOf({ payload: { type: "listener_failed", listener: `${host}/ticket`, signal: "s2", error: why } }, host), "the list is read when the node says so");
  let held = heldOf(signals, host);
  assert.deepEqual(held, [{ id: "s2", step: "ticket", source: "a call to its hook", at: 200, why }]);
  assert.equal(heldWords(held.length), "1 held for you");
  // Let through: the node queues it again; the worker begins its run, and the Runs pane says who began it.
  signals = [signal("s2", "queued", { at: 200 }), signal("s1", "done", { at: 100 })];
  held = heldOf(signals, host);
  assert.deepEqual(held, []);
  assert.equal(heldWords(held.length), null, "nothing is drawn while nothing is held");
  assert.equal(runsPaneRows([{ id: "r9", status: "running", number: 9, scope: "workspace", workflow: stored.id, workflow_name: stored.name, revision: 3, queued_at: 9, started_by: { by: "event", event: "hook" } }])[0].startedBy, "by hook");
  // A goal that listens holds its own.
  assert.deepEqual(heldOf([signal("s3", "held", { listener: "goal:01GOALWEEKLY/ticket" })], hostOf({ goal: "01GOALWEEKLY" })).map((h) => h.id), ["s3"]);
  const chip = src("../views/_workflow/HeldSignals.tsx");
  assert.ok(chip.includes("api.signals(signalsQuery(asked), s)") && chip.includes("api.releaseSignal(id)") && chip.includes("taken(flying.current, id)"), "one press lets one signal through");
  assert.ok(chip.includes("useAsync((s) => (asked === null") && src("../views/_work/useAsync.ts").includes("useReloadOnReconnect(reload)"), "what was held while the node was away is read when it comes back: the chip reads through the hook that reads again");
  assert.ok(src("../views/WorkflowDesigner.tsx").includes("<HeldSignals host={{ workflow: id }}") && src("../views/_goal/GoalHeader.tsx").includes("<HeldSignals host={{ goal: goal.id }}"));
});
