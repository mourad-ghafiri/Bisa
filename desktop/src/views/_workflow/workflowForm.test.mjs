/**
 * The start form, tested where it lives.
 *
 * Run with `npm test` from `desktop/`.
 */

import { strict as assert } from "node:assert";
import { test } from "node:test";

import { readFileSync } from "node:fs";

import { accountChoices, initialValues, inputHint, toRequest, validateInputs, nextInputName } from "./workflowForm.mjs";

const inputs = [
  { name: "who", label: "Who", kind: "text", required: true },
  { name: "n", label: "N", kind: "number", default: 3 },
  { name: "ok", label: "Ok", kind: "bool" },
  { name: "env", label: "Env", kind: "choice", options: ["prod", "staging"], default: "staging" },
  { name: "owner", label: "Owner", kind: "assignee" },
  { name: "proj", label: "Project", kind: "project" },
];

test("initial values are the defaults, else the kind's empty", () => {
  assert.deepEqual(initialValues(inputs), { who: "", n: 3, ok: false, env: "staging", owner: "", proj: "" });
  assert.deepEqual(initialValues(null), {});
});

test("a required input left blank is the one error a blank produces", () => {
  const errors = validateInputs(inputs, initialValues(inputs));
  assert.deepEqual(errors, { who: "Required." });
});

test("each kind refuses what it cannot hold", () => {
  const errors = validateInputs(inputs, {
    who: "me",
    n: "three",
    ok: "yes",
    env: "dev",
    owner: "bob",
    proj: "not-a-ulid",
  });
  assert.equal(errors.n, "A number.");
  assert.equal(errors.ok, "Yes or no.");
  assert.equal(errors.env, "One of the options.");
  assert.match(errors.owner, /agent:<id>/);
  assert.equal(errors.proj, "A project id.");
  assert.deepEqual(
    validateInputs(inputs, {
      who: "me",
      n: "4",
      ok: true,
      env: "prod",
      owner: "agent:developer",
      proj: "01J0000000000000000000000A",
    }),
    {},
  );
});

test("the request types every value by kind and drops blanks", () => {
  assert.deepEqual(toRequest(inputs, { who: " me ", n: "4", ok: true, env: "prod", owner: "", proj: null }), {
    who: "me",
    n: 4,
    ok: true,
    env: "prod",
  });
});

test("a new input takes the first free name", () => {
  assert.equal(nextInputName([]), "input");
  assert.equal(nextInputName([{ name: "input" }]), "input-2");
  assert.equal(nextInputName([{ name: "input" }, { name: "input-2" }]), "input-3");
  assert.equal(nextInputName([{ name: "topic" }]), "input");
  assert.equal(nextInputName(null), "input");
});

test("an account is one of the connector's accounts by its id, and travels as the word it is", () => {
  const who = [{ name: "who", label: "Account", kind: "account", connector: "slack", required: true }];
  assert.deepEqual(validateInputs(who, { who: "01ARZ3NDEKTSV4RRFFQ69G5FAV" }), {});
  assert.deepEqual(validateInputs(who, { who: "work" }), { who: "One of the connector's accounts." });
  assert.deepEqual(validateInputs(who, { who: "" }), { who: "Required." });
  assert.deepEqual(validateInputs([{ ...who[0], required: false }], { who: "" }), {}, "left blank, the connector's default account is the node's to choose");
  assert.deepEqual(toRequest(who, { who: " 01ARZ3NDEKTSV4RRFFQ69G5FAV " }), { who: "01ARZ3NDEKTSV4RRFFQ69G5FAV" });
  assert.deepEqual(initialValues(who), { who: "" });
});

test("an account input offers this machine's accounts of its connector, the default first, each by its label", () => {
  const accounts = [
    { id: "01ARZ3NDEKTSV4RRFFQ69G5FA1", label: "Support", default: false },
    { id: "01ARZ3NDEKTSV4RRFFQ69G5FA2", label: "Work", default: true },
    { id: "01ARZ3NDEKTSV4RRFFQ69G5FA3", label: "" },
  ];
  assert.deepEqual(accountChoices(accounts), [
    { id: "01ARZ3NDEKTSV4RRFFQ69G5FA2", label: "Work", isDefault: true },
    { id: "01ARZ3NDEKTSV4RRFFQ69G5FA1", label: "Support", isDefault: false },
    { id: "01ARZ3NDEKTSV4RRFFQ69G5FA3", label: "01ARZ3NDEKTSV4RRFFQ69G5FA3", isDefault: false },
  ]);
  assert.equal(accounts[0].label, "Support", "the list handed over is left as it was");
  for (const none of [null, undefined, []]) assert.deepEqual(accountChoices(none), []);
});

test("the line under a field says whether it must be given, what picking a project does, and what is wrong — in that order", () => {
  assert.equal(inputHint({ kind: "text", required: true }, undefined, "goal"), "Required.");
  assert.equal(inputHint({ kind: "text" }, undefined, "goal"), "Optional.");
  assert.equal(inputHint({ kind: "number", required: true }, "A number.", "workspace"), "Required. A number.");
  // A project given to a goal's work is attached to the goal by the start; a run in the workspace attaches nothing.
  assert.equal(inputHint({ kind: "project", required: true }, undefined, "goal"), "Required. Picking one attaches it to the goal when the run starts: you say where the work is done.");
  assert.equal(inputHint({ kind: "project" }, undefined, "workspace"), "Optional. The steps that read it work there. A run in the workspace attaches nothing.");
  assert.equal(inputHint({ kind: "project", required: true }, "A project id.", "goal"), "Required. Picking one attaches it to the goal when the run starts: you say where the work is done. A project id.");
  assert.equal(inputHint({ kind: "project", required: true }, "Required.", "workspace").endsWith("attaches nothing. Required."), true, "what is wrong is the last word");
});

test("the form is paint: the hint, the accounts and the home are the model's, and every dialog says whom its run is for", () => {
  const read = (name) => readFileSync(new URL(name, import.meta.url), "utf8");
  const form = read("./InputsForm.tsx");
  assert.ok(form.includes("inputHint(i, errors[i.name], home)") && form.includes("accountChoices(detail.data?.accounts)"));
  assert.ok(!form.includes('"Required."') && !/\? "[A-Z][a-z]+\."/.test(form), "no sentence is written in the component");
  assert.ok(form.includes('case "account":'), "an account has its own field: no id is typed by hand");
  assert.ok(form.includes("ws.projects.map("), "a project input offers every project of the workspace");
  for (const [dialog, home] of [["./StartRunDialog.tsx", "goal"], ["../_studio/PendingAsk.tsx", "goal"], ["./RunWorkflowDialog.tsx", "workspace"], ["./TurnOnDialog.tsx", "workspace"]]) {
    assert.ok(read(dialog).includes(`home="${home}"`), `${dialog} is for a run of the ${home}`);
  }
});
