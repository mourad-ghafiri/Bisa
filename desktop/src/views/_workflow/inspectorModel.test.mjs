/**
 * The panel's facts. Run with
 * `node --test --import ./src/i18n/preload.mjs desktop/src/views/_workflow/inspectorModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { inspectorView, readOnlyReason } from "./inspectorModel.mjs";
import { flowRef, stepSelection } from "./selectionModel.mjs";

const wf = {
  name: "Release",
  steps: [
    { id: "start", name: "Start", kind: "start", on: { event: "manual" }, then: [{ to: "draft" }] },
    { id: "draft", name: "Draft the note", kind: "agent", instructions: "", then: [{ to: "read" }] },
    { id: "read", name: "Read it", kind: "human", prompt: "?", then: [{ to: "done" }] },
    { id: "done", name: "", kind: "end" },
  ],
};
const problems = [
  { step: "draft", kind: "unknown_assignee", text: { id: "problem-unknown-assignee-step-names-who-not-workspace", args: {} } },
  { step: null, kind: "no_start", text: { id: "problem-no-start", args: {} } },
];

test("the read-only reason is the first that holds, in priority, with the door that answers it", () => {
  assert.equal(readOnlyReason(false, { closed: true }), null, "nothing while it may be edited");
  assert.deepEqual(readOnlyReason(true, { closed: true, runLive: true }), { reason: "This goal is closed. Its workflow stays as it was.", door: null });
  assert.deepEqual(readOnlyReason(true, { runLive: true, proposed: true }), { reason: "A run is going. The steps are read-only until it finishes.", door: null });
  assert.equal(readOnlyReason(true, { runningIn: 1 }).reason, "Running in 1 goal. Read-only until that run finishes.");
  assert.equal(readOnlyReason(true, { runningIn: 3, runFinished: true }).reason, "Running in 3 goals. Read-only until those runs finish.");
  assert.equal(readOnlyReason(true, { runningIn: 0, runFinished: true }).reason, "A finished run is history. It is read-only.");
  assert.deepEqual(readOnlyReason(true, { proposed: true, ownDesign: true }), { reason: "Proposed by the Workflow Agent. Edit the steps to make it this goal's own.", door: { act: "edit", label: "Edit the steps" } });
  assert.deepEqual(readOnlyReason(true, { ownDesign: true, library: true }), { reason: "Saved for this goal. Edit the steps before a run starts.", door: { act: "edit", label: "Edit the steps" } });
  assert.deepEqual(readOnlyReason(true, { library: true }), { reason: "From the library. It is edited in the designer.", door: { act: "designer", label: "Open in the designer" } });
  assert.deepEqual(readOnlyReason(true, {}), { reason: "Read-only here.", door: null });
  assert.deepEqual(readOnlyReason(true, null), { reason: "Read-only here.", door: null });
});

test("nothing picked: the workflow's own view — its sections in order, every problem, and whether it can run", () => {
  const v = inspectorView({ selection: null, workflow: wf, problems });
  assert.deepEqual([v.kind, v.title, v.subtitle, v.frozen, v.readOnly, v.remove], ["workflow", "Workflow", "Release", false, null, false]);
  assert.deepEqual(v.sections, ["identity", "runs", "inputs", "problems"]);
  assert.deepEqual([v.problems.count, v.problems.canRun, v.problems.list], [2, false, problems]);
  const clean = inspectorView({ selection: null, workflow: { ...wf, name: "" } });
  assert.deepEqual([clean.subtitle, clean.problems.canRun], ["Untitled workflow", true]);
  assert.equal(inspectorView({ selection: null, workflow: wf, unreadable: "offline" }).problems.canRun, false, "a design the node could not read is not known to run");
  const frozen = inspectorView({ selection: null, workflow: wf, edit: { readOnly: true }, why: { library: true } });
  assert.deepEqual([frozen.frozen, frozen.readOnly.door.act], [true, "designer"]);
  assert.equal(inspectorView({ selection: stepSelection("gone"), workflow: wf }).kind, "workflow", "a step the workflow has not got is the workflow's view");
});

test("a step: its name and kind, its own problems only, the fold and the danger footer, and the amendment's grain", () => {
  const v = inspectorView({ selection: stepSelection("draft"), workflow: wf, problems });
  assert.deepEqual([v.kind, v.title, v.subtitle, v.frozen, v.stepStarted, v.remove], ["step", "Draft the note", "Agent", false, false, true]);
  assert.deepEqual(v.sections, ["identity", "kind", "flow", "problems", "danger"]);
  assert.deepEqual([v.problems.count, v.problems.list[0].kind, v.problems.canRun], [1, "unknown_assignee", false]);
  assert.equal(v.run, null);
  const quiet = inspectorView({ selection: stepSelection("read"), workflow: wf, problems });
  assert.deepEqual(quiet.sections, ["identity", "kind", "flow", "danger"], "no problems of its own, no Problems section");
  assert.equal(inspectorView({ selection: stepSelection("done"), workflow: wf }).title, "done", "a step with no name is its id");
  const ro = inspectorView({ selection: stepSelection("draft"), workflow: wf, edit: { readOnly: true }, why: { runLive: true } });
  assert.deepEqual([ro.frozen, ro.stepStarted, ro.remove, ro.sections.includes("danger"), ro.readOnly.reason], [true, false, false, false, "A run is going. The steps are read-only until it finishes."]);
  // An amendment: the started step is frozen with its own note, the rest is not.
  const started = inspectorView({ selection: stepSelection("draft"), workflow: wf, edit: { readOnly: false, editable: new Set(["read", "done"]) } });
  assert.deepEqual([started.frozen, started.stepStarted, started.readOnly, started.remove], [true, true, null, false]);
  const open = inspectorView({ selection: stepSelection("read"), workflow: wf, edit: { readOnly: false, editable: new Set(["read", "done"]) } });
  assert.deepEqual([open.frozen, open.stepStarted], [false, false]);
});

test("a step in a run: its record, whose move it is, its acts and its progress row — first among its sections", () => {
  const run = { workflow: wf, steps: { draft: { state: { state: "done" }, started_at: 10, finished_at: 40, output: { ok: true } }, read: { state: { state: "waiting" }, started_at: 50 } } };
  const read = inspectorView({ selection: stepSelection("read"), workflow: wf, run, edit: { readOnly: true }, why: { runLive: true } });
  assert.deepEqual(read.sections, ["run", "identity", "kind", "flow"]);
  assert.deepEqual([read.run.yours, read.run.actions, read.run.row.holder, read.run.row.state.state], [true, ["answer", "done"], "you", "waiting"]);
  const draft = inspectorView({ selection: stepSelection("draft"), workflow: wf, run });
  assert.deepEqual([draft.run.yours, draft.run.actions, draft.run.row.duration, draft.run.row.output], [false, [], "30s", { ok: true }]);
  assert.equal(inspectorView({ selection: stepSelection("start"), workflow: wf, run: { ...run, steps: { ...run.steps, start: { state: { state: "done" } } } } }).run.row, null, "a start is no row of work");
  assert.equal(inspectorView({ selection: stepSelection("done"), workflow: wf, run }).run, null, "no record yet, no run section");
});

test("a flow: its ends in the subtitle, its facts, and Remove while it may be edited and still exists", () => {
  const v = inspectorView({ selection: flowRef("draft", "read"), workflow: wf });
  assert.deepEqual([v.kind, v.title, v.subtitle, v.frozen, v.remove, v.sections], ["flow", "Flow", "Draft the note → Read it", false, true, ["flow", "danger"]]);
  assert.deepEqual([v.flow.exists, v.flow.word, v.flow.loop], [true, "Then", false]);
  const gone = inspectorView({ selection: flowRef("read", "draft"), workflow: wf });
  assert.deepEqual([gone.flow.exists, gone.remove, gone.sections], [false, false, ["flow"]]);
  assert.equal(inspectorView({ selection: flowRef("draft", "zz"), workflow: wf }).subtitle, "Draft the note → zz", "an end the workflow has not got is named by its id");
  const ro = inspectorView({ selection: flowRef("draft", "read"), workflow: wf, edit: { readOnly: true }, why: { runFinished: true } });
  assert.deepEqual([ro.frozen, ro.remove, ro.readOnly.reason], [true, false, "A finished run is history. It is read-only."]);
  const started = inspectorView({ selection: flowRef("draft", "read"), workflow: wf, edit: { editable: new Set(["read"]) } });
  assert.deepEqual([started.frozen, started.remove], [true, false], "a flow out of a started step may not be cut");
});

test("several picked: a count, a hint, the steps that still exist, and no form", () => {
  const v = inspectorView({ selection: { kind: "many", steps: ["draft", "zz", "read"], flows: [flowRef("start", "draft")] }, workflow: wf });
  assert.deepEqual([v.kind, v.title, v.subtitle, v.sections, v.remove, v.frozen], ["many", "4 selected", "Pick one to edit it. Delete removes everything selected; undo brings it back.", ["many"], false, false]);
  assert.deepEqual([v.many.n, v.many.steps.map((s) => s.id), v.many.flows.length], [4, ["draft", "read"], 1]);
  assert.equal(inspectorView({ selection: { kind: "many", steps: ["draft", "read"], flows: [] }, workflow: wf, edit: { readOnly: true } }).frozen, true);
});
