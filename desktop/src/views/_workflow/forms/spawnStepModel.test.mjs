/**
 * What a `spawn` step gives the workflow it opens a goal on.
 * Run with `node --test --import ./src/i18n/preload.mjs src/views/_workflow/forms/spawnStepModel.test.mjs`
 * from `desktop/`.
 */

import { strict as assert } from "node:assert";
import { test } from "node:test";

import { give, givenRows, leftOut, notAskedFor, onWorkflow } from "./spawnStepModel.mjs";

const asked = [
  { name: "report", label: "The bug, as reported", kind: "text", required: true },
  { name: "project", label: "The project to work in", kind: "project", required: true },
  { name: "rounds", label: "Rounds", kind: "number", required: true, default: 3 },
  { name: "notes", label: "", kind: "text", required: false },
];
const step = { id: "followup", name: "Open the follow-up", kind: "spawn", statement_template: "Fix {steps.cause.output.root}", workflow: "01WF", wait: false };

test("a row per input the child asks for says whether its run needs it and what the step gives", () => {
  const given = give(step, "report", "{steps.cause.output.root}");
  assert.deepEqual(givenRows(given, asked), [
    { input: "report", label: "The bug, as reported", required: true, template: "{steps.cause.output.root}" },
    { input: "project", label: "The project to work in", required: true, template: null },
    { input: "rounds", label: "Rounds", required: false, template: null },
    { input: "notes", label: "notes", required: false, template: null },
  ]);
  assert.deepEqual(givenRows(null, asked).map((row) => row.template), [null, null, null, null]);
});

test("what its run needs and the step does not give is named, and a default or a gift fills it", () => {
  assert.deepEqual(leftOut(step, asked), ["report", "project"]);
  const given = give(give(step, "report", "r"), "project", "{inputs.project}");
  assert.deepEqual(leftOut(given, asked), []);
  assert.deepEqual(given.inputs, { report: "r", project: "{inputs.project}" });
  // The step is not changed in place, and its other fields are kept.
  assert.equal(step.inputs, undefined);
  assert.equal(given.statement_template, step.statement_template);
});

test("a gift taken back is no key, and a step that gives nothing says nothing", () => {
  const given = give(step, "report", "r");
  assert.deepEqual(give(given, "report", "  "), step);
  assert.deepEqual(give(given, "report", null), step);
  assert.equal("inputs" in give(given, "report", ""), false);
});

test("what the child does not ask for is shown to be removed", () => {
  const given = give(give(step, "report", "r"), "reprot", "r");
  assert.deepEqual(notAskedFor(given, asked), ["reprot"]);
  assert.deepEqual(notAskedFor(give(given, "reprot", null), asked), []);
});

test("on another workflow the step keeps what that one asks for too, and lets the rest go", () => {
  const given = give(give(give(step, "report", "r"), "project", "p"), "rounds", "5");
  const other = [{ name: "report", label: "Report", kind: "text", required: true }, { name: "symptom", label: "Symptom", kind: "text", required: true }];
  const moved = onWorkflow(given, "01OTHER", other);
  assert.equal(moved.workflow, "01OTHER");
  assert.deepEqual(moved.inputs, { report: "r" });
  assert.deepEqual(leftOut(moved, other), ["symptom"]);
  // Handed to the Workflow Agent, there is no workflow to give anything to.
  const handed = onWorkflow(given, null, []);
  assert.equal(handed.workflow, null);
  assert.equal("inputs" in handed, false);
  assert.deepEqual(given.inputs, { report: "r", project: "p", rounds: "5" }, "the step it came from is as it was");
  // A workflow the picker named before reading it: nothing is let go on a guess.
  const unread = onWorkflow(given, "01UNREAD", null);
  assert.equal(unread.workflow, "01UNREAD");
  assert.deepEqual(unread.inputs, given.inputs);
  assert.deepEqual(notAskedFor(unread, other), ["project", "rounds"], "once it is read, what it does not ask for is shown to be removed");
  assert.equal(onWorkflow(given, undefined, other).workflow, null, "a reference not chosen is `null`, never absent nor empty");
});
