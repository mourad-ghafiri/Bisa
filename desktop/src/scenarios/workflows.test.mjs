/**
 * A workflow, as a person makes and runs one: a `spawn` step drawn in the
 * inspector — the child picked, what it is given, what is left out, who
 * carries it; a condition typed; a save the node refuses because somebody
 * saved first, and one it refuses for another reason; the run form, asked by
 * hand and as a test, with a project and an account; *Turn on*, which asks
 * only what no default fills; the picker and the library's headings. No
 * window is opened and nothing is sent: the scenario steps the models the
 * designer, the forms and the dialogs step, and holds every body they build
 * to the schema the node reads it by.
 *
 * Run with `node --test --import ./src/i18n/preload.mjs desktop/src/scenarios/workflows.test.mjs`.
 */

import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { bodyOf, definitionBody, dirty, edit, keepMine, open, putBody, refusalOutcome, remoteAction, remoteLoaded, saveDue, saveRequest, saveStarted, settle, statusLine, takeTheirs } from "../views/_workflow/designerSession.mjs";
import { fixedWords, inputNames, picked, toggleInput, valueOf, withFixed } from "../views/_workflow/forms/assigneeRefModel.mjs";
import { conditionOf, freshCondition, parseValue, valueText } from "../views/_workflow/forms/conditionModel.mjs";
import { give, givenRows, leftOut, notAskedFor, onWorkflow } from "../views/_workflow/forms/spawnStepModel.mjs";
import { listeningInputs, listeningNeeds, startInputs } from "../views/_workflow/forms/startForm.mjs";
import { domainLabel, groupByDomain } from "../views/_workflow/libraryModel.mjs";
import { neededInputs, readBudget, turnOnBody } from "../views/_workflow/listeningModel.mjs";
import { BY_HAND, askedInputs, runEntries, runRefusal, runRequest } from "../views/_workflow/runDialogModel.mjs";
import { PROBLEM_KIND_LABEL, STEP_KINDS, blankStep, blankWorkflow } from "../views/_workflow/stepKinds.mjs";
import { accountChoices, initialValues, inputHint, toRequest, validateInputs } from "../views/_workflow/workflowForm.mjs";
import { replaceStep, upstreamOf } from "../views/_workflow/workflowGraph.mjs";
import { installedRow, installedWords, optionLabel, templateSlugOf, templateValue, unlistedLabel } from "../views/_workflow/workflowPickerModel.mjs";
import { misfits } from "./schemaFit.mjs";

const read = (name) => readFileSync(new URL(name, import.meta.url), "utf8");
const SCHEMA = JSON.parse(read("../../api-schema.json"));
/** The shapes the core holds to their keys by hand (`bodiesFitTheSchema.test.mjs`). */
const HELD_BY_HAND = ["Step", "InputDef", "StartOn", "WaitFor", "Boundary", "BoundaryOn"];
const fits = (name, body) => misfits(SCHEMA, name, body, { closed: HELD_BY_HAND });

const PROJECT = "01ARZ3NDEKTSV4RRFFQ69G5FAX";
const ACCOUNT = "01ARZ3NDEKTSV4RRFFQ69G5FAY";
const CHILD = "01ARZ3NDEKTSV4RRFFQ69G5FAZ";
const OTHER = "01ARZ3NDEKTSV4RRFFQ69G5FB0";
const KEY = "ab".repeat(32);

/** What the child's workflow asks of whoever starts it. */
const childAsks = [
  { name: "report", label: "The bug, as reported", kind: "text", required: true },
  { name: "project", label: "Where the fix is made", kind: "project", required: true },
  { name: "rounds", label: "Review rounds", kind: "number", required: true, default: 2 },
  { name: "notes", label: "", kind: "text", required: false },
];

/** A parent: a start by hand, an agent that finds the cause, a spawn that opens the fix. */
function parent() {
  const wf = blankWorkflow("Triage and fix");
  const start = { ...wf.steps[0], then: [{ to: "cause" }] };
  const cause = { ...blankStep("agent", "cause"), name: "Find the cause", instructions: "Find what breaks: {inputs.report}", then: [{ to: "fix" }] };
  const fix = { ...blankStep("spawn", "fix"), name: "Open the fix", statement_template: "Fix {steps.cause.output.root}" };
  return {
    ...wf,
    inputs: [
      { name: "report", label: "Report", kind: "text", required: true },
      { name: "project", label: "Project", kind: "project", required: true },
      { name: "owner", label: "Owner", kind: "assignee", required: false },
    ],
    steps: [start, cause, fix],
  };
}
const stepOf = (wf, id) => wf.steps.find((s) => s.id === id);

test("a spawn step is a start by hand made by a step: it is asked what the child asks, and what its run needs and the step does not give is named", () => {
  assert.ok(STEP_KINDS.some((k) => k.kind === "spawn"), "one of the eighteen");
  let wf = parent();
  let fix = stepOf(wf, "fix");
  assert.equal(fix.workflow ?? null, null, "born handed to the Workflow Agent: a reference not chosen is null, never an empty string");
  assert.deepEqual(fits("NewWorkflowBody", definitionBody(wf)), []);

  // The child is picked: a row per input it asks for, the ones its run needs marked.
  fix = onWorkflow(fix, CHILD, childAsks);
  assert.deepEqual(givenRows(fix, childAsks).map((r) => [r.input, r.label, r.required, r.template]), [
    ["report", "The bug, as reported", true, null],
    ["project", "Where the fix is made", true, null],
    ["rounds", "Review rounds", false, null],
    ["notes", "notes", false, null],
  ]);
  assert.deepEqual(leftOut(fix, childAsks), ["report", "project"], "a default fills `rounds`; `notes` is optional");
  assert.equal(PROBLEM_KIND_LABEL.spawn_input.length > 8, true, "the node's refusal has its words in the problems list");

  // Each is given as a template of this run.
  fix = give(fix, "report", "{steps.cause.output.root}");
  assert.deepEqual(leftOut(fix, childAsks), ["project"]);
  fix = give(fix, "project", "{inputs.project}");
  fix = give(fix, "reprot", "typed in a hurry");
  assert.deepEqual(leftOut(fix, childAsks), []);
  assert.deepEqual(notAskedFor(fix, childAsks), ["reprot"], "shown to be removed, never dropped in silence");
  fix = give(fix, "reprot", null);
  assert.deepEqual(fix.inputs, { report: "{steps.cause.output.root}", project: "{inputs.project}" });
  assert.deepEqual(upstreamOf(wf, "fix").map((s) => s.id), ["start", "cause"], "what its templates may read");

  // Who carries the child: fixed assignees from the picker, an input of the run beside them.
  fix = { ...fix, assignees: withFixed(fix.assignees, ["agent:developer", `human:${KEY}`]) };
  fix = { ...fix, assignees: toggleInput(fix.assignees, "owner") };
  fix = { ...fix, assignees: withFixed(fix.assignees, ["team:01TEAM"]) };
  assert.deepEqual(fix.assignees, [{ team: "01TEAM" }, { input: "owner" }], "the picker replaced its own, and the input rode along");
  assert.deepEqual([fixedWords(fix.assignees), inputNames(fix.assignees)], [["team:01TEAM"], ["owner"]]);

  wf = replaceStep(wf, "fix", fix);
  assert.deepEqual(fits("NewWorkflowBody", definitionBody(wf)), [], "the step as the form left it is the node's shape, its `inputs` among its keys");
  assert.deepEqual(fits("NewWorkflowBody", definitionBody(replaceStep(wf, "fix", { ...fix, given: fix.inputs }))), ["$.steps[2].given: a key `Step` does not declare"], "the guard bites");

  // Another child: what it asks for too is kept, the rest let go; one not read yet keeps everything.
  const otherAsks = [{ name: "report", label: "Report", kind: "text", required: true }, { name: "symptom", label: "Symptom", kind: "text", required: true }];
  const moved = onWorkflow(fix, OTHER, otherAsks);
  assert.deepEqual([moved.workflow, moved.inputs, leftOut(moved, otherAsks)], [OTHER, { report: "{steps.cause.output.root}" }, ["symptom"]]);
  assert.deepEqual(onWorkflow(fix, OTHER, null).inputs, fix.inputs);
  const handed = onWorkflow(fix, null, []);
  assert.deepEqual([handed.workflow, "inputs" in handed], [null, false]);
  assert.deepEqual(fits("NewWorkflowBody", definitionBody(replaceStep(wf, "fix", handed))), []);

  const form = read("../views/_workflow/forms/SpawnStepForm.tsx");
  for (const call of ["givenRows(step, asked)", "leftOut(step, asked)", "notAskedFor(step, asked)", "onWorkflow(step, id, picked?.inputs ?? null)", "give(step, r.input, e.target.value)", "withFixed(step.assignees, next)", "toggleInput(step.assignees, i.name)"]) assert.ok(form.includes(call), call);
  assert.ok(form.includes("aria-invalid={missing.has(r.input)}"), "a row left out is drawn as one");
});

test("one assignee or an input, and a condition typed: what a field shows is what it would write back", () => {
  for (const ref of [{ agent: "developer" }, { team: "01TEAM" }, { human: KEY }, { input: "owner" }]) assert.deepEqual(picked(valueOf(ref)), ref);
  assert.equal(picked(""), null);
  const upstream = [{ id: "tests", name: "Tests", kind: "check" }];
  const first = freshCondition(upstream, []);
  assert.deepEqual(first, { condition: "outcome", step: "tests", passed: true });
  const both = conditionOf("all", upstream, [], first);
  assert.deepEqual(both, { condition: "all", of: [first] }, "wrapped, not lost");
  // The text `5` stays text: shown quoted, so the next keystroke does not make it a number.
  assert.deepEqual([parseValue(valueText("5")), parseValue(valueText(5))], ["5", 5]);
});

test("a save the node refuses: somebody saved first is a choice with two exits; any other refusal is said beside the draft", () => {
  const stored = (revision, name = "Triage and fix") => ({ ...parent(), id: CHILD, name, revision, origin: { origin: "workspace" }, author: KEY, created_at: 1 });
  let s = open(stored(3));
  s = edit(s, { ...bodyOf(stored(3)), name: "Triage, then fix" }, 1000);
  assert.equal(saveDue(s, 1000, 800), 800, "after the quiet delay");
  const sent = saveRequest(s);
  assert.deepEqual([sent.kind, sent.revision], ["update", 3]);
  assert.deepEqual(fits("PutWorkflowBody", putBody(sent.body, sent.revision)), []);
  assert.deepEqual(Object.keys(putBody({ ...sent.body, selected: "fix", id: CHILD }, 3)).sort(), ["decision_making", "description", "inputs", "name", "revision", "steps", "tags"], "by name: what the canvas carries beside the definition stays on the canvas");

  // The node answers 409 and the stored copy is at 4: a conflict.
  const moved = "the stored copy has moved: you edited revision 3, it is at 4";
  s = settle(saveStarted(s, sent.body), refusalOutcome({ status: 409, message: moved, body: { error: moved } }, sent.revision, stored(4, "Theirs")));
  assert.deepEqual([s.status, statusLine(s), saveRequest(s)], ["conflict", "conflict — somebody saved first", null]);
  // Keep mine: my body, on their revision.
  const mine = keepMine(s);
  assert.deepEqual([saveRequest(mine).revision, saveRequest(mine).body.name], [4, "Triage, then fix"]);
  const landed = settle(saveStarted(mine, saveRequest(mine).body), { kind: "stored", workflow: stored(5, "Triage, then fix"), problems: [] });
  assert.deepEqual([landed.status, landed.base.revision, dirty(landed), statusLine(landed)], ["idle", 5, false, "saved"]);
  // Take theirs: clean on theirs, and undo still holds mine.
  const theirs = takeTheirs(s);
  assert.deepEqual([theirs.base.revision, dirty(theirs), saveRequest(theirs)], [4, false, null]);

  // A 409 over a copy nobody changed is no conflict: the node's own sentence, the draft kept, tried again later.
  const held = "the public hook start `ticket` is still answered by a listening host: turn it off first";
  let h = edit(open(stored(3)), { ...bodyOf(stored(3)), name: "Without its hook" }, 1000);
  h = settle(saveStarted(h, saveRequest(h).body), refusalOutcome({ status: 409, message: held, body: { error: held } }, 3, stored(3)));
  assert.deepEqual([h.status, h.conflict, statusLine(h)], ["failed", null, `not saved: ${held}`]);

  // Another writer's save heard on the bus: a clean designer stands on it, an edited one is asked.
  const clean = open(stored(3));
  assert.equal(remoteAction(clean, { type: "workflow_changed", workflow: CHILD, revision: 4 }), "reload");
  assert.equal(remoteAction(clean, { type: "workflow_changed", workflow: CHILD, revision: 3 }), "ignore", "its own save, echoed back");
  assert.equal(remoteAction(clean, { type: "workflow_deleted", workflow: CHILD }), "leave");
  assert.equal(remoteAction(clean, { type: "workflow_changed", workflow: OTHER, revision: 9 }), "ignore");
  assert.deepEqual([remoteLoaded(clean, stored(4)).base.revision, remoteLoaded(clean, stored(4)).status], [4, "idle"]);
  assert.equal(remoteLoaded(edit(clean, { ...bodyOf(stored(3)), name: "Mine" }, 1), stored(4)).status, "conflict");
});

test("the run form: asked by hand or as a test, typed by kind, with what picking a project does said under it", () => {
  const wf = {
    ...blankWorkflow("Weekly digest"),
    inputs: [
      { name: "audience", label: "Audience", kind: "text", required: true },
      { name: "count", label: "How many", kind: "number", required: false, default: 5 },
      { name: "draft", label: "Draft only", kind: "bool", required: false },
      { name: "tone", label: "Tone", kind: "choice", options: ["plain", "warm"], required: false, default: "plain" },
      { name: "project", label: "Project", kind: "project", required: false },
      { name: "slack", label: "Slack", kind: "account", connector: "slack", required: true },
      { name: "ticket", label: "Ticket", kind: "text", required: true },
    ],
  };
  wf.steps = [
    { ...wf.steps[0], then: [{ to: "write" }] },
    { ...blankStep("start", "ticket"), name: "A ticket arrives", on: { event: "hook" }, inputs: { ticket: "{event.payload.body}" }, then: [{ to: "write" }] },
    { ...blankStep("agent", "write"), instructions: "Write for {inputs.audience}" },
  ];
  assert.deepEqual(runEntries(wf).map((e) => [e.id, e.test]), [[BY_HAND, false], ["ticket", true]]);
  assert.deepEqual(askedInputs(wf, BY_HAND), { asked: wf.inputs.map((i) => i.name), mapped: 0 });
  assert.deepEqual(askedInputs(wf, "ticket"), { asked: ["audience", "count", "draft", "tone", "project", "slack"], mapped: 1 }, "the event fills the ticket");

  // The form opens on the defaults; what is wrong is said under its field.
  let values = initialValues(wf.inputs);
  assert.deepEqual(values, { audience: "", count: 5, draft: false, tone: "plain", project: "", slack: "", ticket: "" });
  const refused = runRequest(wf, BY_HAND, { ...values, count: "many" }, "");
  assert.deepEqual(refused, { kind: "refused", errors: { audience: "Required.", count: "A number.", slack: "Required.", ticket: "Required." }, payloadError: null });
  assert.equal(inputHint(wf.inputs[0], refused.errors.audience, "workspace"), "Required. Required.");
  assert.equal(inputHint(wf.inputs[4], undefined, "workspace"), "Optional. The steps that read it work there. A run in the workspace attaches nothing.");
  assert.equal(inputHint(wf.inputs[4], undefined, "goal"), "Optional. Picking one attaches it to the goal when the run starts: you say where the work is done.");
  // An account is picked among this machine's accounts of the input's connector, the default first.
  assert.deepEqual(accountChoices([{ id: "01ARZ3NDEKTSV4RRFFQ69G5FB1", label: "Support" }, { id: ACCOUNT, label: "Work", default: true }]).map((a) => [a.id, a.isDefault]), [[ACCOUNT, true], ["01ARZ3NDEKTSV4RRFFQ69G5FB1", false]]);
  assert.deepEqual(validateInputs([wf.inputs[5]], { slack: "work" }), { slack: "One of the connector's accounts." });

  values = { ...values, audience: " the team ", count: "12", draft: true, project: PROJECT, slack: ACCOUNT, ticket: "printer on fire" };
  const byHand = runRequest(wf, BY_HAND, values, "");
  assert.deepEqual(byHand, { kind: "run", inputs: { audience: "the team", count: 12, draft: true, tone: "plain", project: PROJECT, slack: ACCOUNT, ticket: "printer on fire" } }, "typed by kind: a number is a number, a yes a yes");
  assert.deepEqual(fits("StartRunBody", { inputs: byHand.inputs }), []);

  // A test run: the start it begins at, the event as typed, and never an input the mapping fills.
  const tested = runRequest(wf, "ticket", values, '{ "body": "printer on fire" }');
  assert.equal(tested.kind, "test");
  assert.deepEqual([tested.body.start, tested.body.event, "ticket" in tested.body.inputs], ["ticket", { body: "printer on fire" }, false]);
  assert.deepEqual(fits("StartRunBody", tested.body), []);
  assert.equal(runRequest(wf, "ticket", values, "{ not json").kind, "refused");
  assert.deepEqual(runRequest(wf, "nowhere", values, ""), { kind: "refused", errors: {}, payloadError: null });

  // Refused by name: a step reads its goal, and the row is read again.
  const needsGoal = runRefusal({ error: "…", problems: [{ kind: "needs_goal", step: "write" }, { kind: "needs_goal", step: "tell" }] }, "…");
  assert.deepEqual([needsGoal.needsGoal, needsGoal.reread], [true, true]);
  assert.match(needsGoal.words, /write and tell/);
  assert.deepEqual(runRefusal(undefined, "the node said no"), { words: "the node said no", needsGoal: false, reread: false });
});

test("Turn on asks what its events do not supply and no default fills — each one required — and sends it with a ceiling when one is set", () => {
  const inputs = [
    { name: "when", label: "When", kind: "text", required: false },
    { name: "channel", label: "Channel", kind: "text", required: true, default: "general" },
    { name: "audience", label: "Audience", kind: "text", required: true },
    { name: "tone", label: "Tone", kind: "text", required: false },
  ];
  const wf = { ...blankWorkflow("Weekly review"), inputs };
  wf.steps = [
    { ...wf.steps[0], then: [{ to: "write" }] },
    { ...blankStep("start", "weekly"), name: "Every week", on: { event: "schedule", cron: { input: "when" }, tz: "UTC" }, then: [{ to: "write" }] },
    { ...blankStep("agent", "write"), instructions: "Write for {inputs.audience} in {inputs.channel}, {inputs.tone}" },
  ];
  assert.deepEqual(listeningNeeds(wf), ["when", "audience"], "this build's reading of the definition");
  const row = { workflow: { ...wf, id: CHILD, archived: null, origin: { origin: "workspace" } }, listening_needs: ["audience", "when"], starts: [], problems: [], workspace_problems: [], listening: null };
  const needs = neededInputs(row);
  assert.deepEqual(needs.map((d) => [d.name, d.required]), [["when", true], ["audience", true]], "the node's word, in the workflow's order; `when` is optional by hand and required to listen");
  assert.ok(!needs.some((d) => d.name === "channel"), "an input a default fills is never asked: nobody gave it, and it is read at its default");
  assert.deepEqual(neededInputs({ ...row, listening_needs: [...row.listening_needs, "channel"] }).map((d) => d.name), ["when", "audience"], "whatever the row names");
  assert.deepEqual(listeningInputs(wf).map((d) => d.name), needs.map((d) => d.name), "a goal that starts by listening is asked the same");
  assert.equal(startInputs(wf, false), inputs, "and a start by hand every input");

  assert.deepEqual(validateInputs(needs, initialValues(needs)), { when: "Required.", audience: "Required." });
  const ceiling = readBudget({ dollars: "2,50", tokens: "", minutes: "10" });
  assert.deepEqual(ceiling, { budget: { max_usd_cents: 250, max_tokens: null, max_wall_clock_secs: 600 }, errors: {} });
  const body = turnOnBody(toRequest(needs, { when: "0 9 * * 1", audience: "the team" }), ceiling.budget);
  assert.deepEqual(body, { inputs: { when: "0 9 * * 1", audience: "the team" }, budget: { max_usd_cents: 250, max_wall_clock_secs: 600 } });
  assert.deepEqual(fits("ListeningBody", body), []);
  assert.deepEqual(turnOnBody(toRequest([], {}), readBudget(null).budget), {}, "nothing asked, no ceiling: the workspace's default applies");
  assert.equal(Object.keys(readBudget({ dollars: "free" }).errors).length, 1, "a ceiling that is no number is said, and nothing is sent");
});

test("the picker and the library say what each workflow is", () => {
  const row = (id, over = {}, problems = []) => ({ workflow: { id, name: `Workflow ${id}`, archived: null, origin: { origin: "workspace" }, tags: [], ...over }, problems });
  assert.equal(optionLabel(row("a", {}, [{}, {}])), "Workflow a (2 problems)");
  assert.equal(unlistedLabel(row("a", { archived: { at: 1 } }).workflow), "Workflow a (archived)");
  assert.equal(templateSlugOf(templateValue("bug-fix")), "bug-fix");
  const rows = [row("01A"), row("01B", { origin: { origin: "catalog", slug: "bug-fix" } })];
  assert.equal(installedRow({ workflows: [] }, rows, "bug-fix").workflow.id, "01B");
  assert.equal(installedWords("bug-fix", { workflows: [], agents: [], skills: [] }), "bug-fix was already installed — opening it.");
  const groups = groupByDomain([row("a", { tags: ["ops"] }), row("b")], (r) => r.workflow.tags);
  assert.deepEqual(groups.map(([domain]) => domainLabel(domain)), ["general", "ops"]);
});
