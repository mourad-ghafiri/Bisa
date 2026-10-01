import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { mintedBy, shownFor, shownWith } from "./hookSecretsModel.mjs";

const src = (name) => readFileSync(new URL(name, import.meta.url), "utf8");

// Shapes only: these are not secrets, and nothing accepts them.
const ticket = { step: "ticket", path: "/hooks/goal:01G/ticket", secret: "aa".repeat(32) };
const order = { step: "order", path: "/hooks/goal:01G/order", secret: "bb".repeat(32) };

test("what a decision minted joins what is still on screen, in the order it came", () => {
  assert.deepEqual(shownWith([], [ticket]), [ticket]);
  assert.deepEqual(shownWith([ticket], [order]), [ticket, order]);
});

test("nothing minted changes nothing, and is the same list", () => {
  const shown = [ticket];
  assert.equal(shownWith(shown, []), shown);
  assert.equal(shownWith(shown, null), shown);
  assert.equal(shownWith(shown, undefined), shown);
});

test("a hook minted again shows its newest secret alone", () => {
  const rotated = { ...ticket, secret: "cc".repeat(32) };
  assert.deepEqual(shownWith([ticket, order], [rotated]), [order, rotated]);
});

test("what an act minted is what its answer carries — every act that arms a listener, listening again among them", () => {
  assert.deepEqual(mintedBy({ workflow: {}, listeners: [], secrets: [ticket] }), [ticket]);
  assert.deepEqual(mintedBy({ goal: {}, listeners: [], secrets: [] }), []);
  assert.deepEqual(mintedBy({ goal: {} }), [], "an answer that names none minted none");
  assert.deepEqual(mintedBy(null), []);
  // A secret is never disclosed by a read route: an act that drops what it minted loses it for good.
  // Each act that arms a listener either shows what it minted itself or hands it to the store.
  const hands = (file) => /showHookSecrets\(mintedBy\(/.test(src(file));
  assert.ok(hands("./ListeningSwitch.tsx"), "the header's Listen again");
  assert.ok(hands("../_goal/GoalHeader.tsx"), "a goal's Listen again");
  assert.ok(src("./TurnOnDialog.tsx").includes("setSecrets(on.secrets)"), "turning On shows them in its own dialog");
});

test("a rotated secret is shown under the hook it was minted for, and under no other step's form", () => {
  assert.equal(shownFor(ticket, "ticket"), ticket);
  assert.equal(shownFor(ticket, "order"), null, "another hook start picked on the canvas never wears it");
  assert.equal(shownFor(null, "ticket"), null);
  const form = src("./forms/StartStepForm.tsx");
  assert.ok(form.includes("shownFor(shown, step.id)"), "the form asks the model");
  // Every step's form is its own: a draft, a picked word or a shown secret never carries over to the next step picked.
  assert.ok(src("./Inspector.tsx").includes("<KindForm key={step.id}") && src("./Inspector.tsx").includes("<StepCommonForm\n          key={step.id}"), "the inspector keys a step's forms by the step");
});
