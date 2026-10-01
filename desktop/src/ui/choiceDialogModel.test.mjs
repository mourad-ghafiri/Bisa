import assert from "node:assert/strict";
import { test } from "node:test";
import { isOpen, openingChoice, stepChoice } from "./choiceDialogModel.mjs";

const choices = [
  { id: "archive" },
  { id: "forget" },
  { id: "delete", tone: "danger" },
];

test("the focus opens on the first choice that destroys nothing", () => {
  assert.equal(openingChoice(choices), "archive");
  assert.equal(openingChoice([{ id: "delete", tone: "danger" }, { id: "forget" }]), "forget", "never on the destructive one while another stands");
  assert.equal(openingChoice([{ id: "delete", tone: "danger" }]), "delete", "the only choice is where the focus goes");
  assert.equal(openingChoice([{ id: "archive", disabled: "already archived" }, { id: "forget" }]), "forget");
  assert.equal(openingChoice([{ id: "x", disabled: "no" }]), null, "nothing to take: Cancel keeps the focus");
  assert.equal(openingChoice([]), null);
});

test("the arrows walk the choices that can be taken and stop at the ends", () => {
  assert.equal(stepChoice(choices, "archive", 1), "forget");
  assert.equal(stepChoice(choices, "delete", 1), "delete");
  assert.equal(stepChoice(choices, "archive", -1), "archive");
  assert.equal(stepChoice(choices, null, 1), "archive");
  assert.equal(stepChoice(choices, null, -1), "delete");
  const held = [{ id: "a" }, { id: "b", disabled: "adopted" }, { id: "c" }];
  assert.equal(stepChoice(held, "a", 1), "c", "a choice that only says why is stepped over");
  assert.equal(isOpen(held[1]), false);
  assert.equal(isOpen({ id: "a", disabled: null }), true);
  assert.equal(stepChoice([], null, 1), null);
});
