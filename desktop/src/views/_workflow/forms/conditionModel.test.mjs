/**
 * A condition as the editor makes and edits it. Run with
 * `node --test --import ./src/i18n/preload.mjs src/views/_workflow/forms/conditionModel.test.mjs`
 * from `desktop/`.
 */

import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { COMBINATORS, CONDITIONS, MAX_CONDITION_DEPTH } from "../stepKinds.mjs";
import { conditionOf, freshCondition, hourOf, isGroup, mayNest, offeredKinds, offeredWith, parseValue, valueText } from "./conditionModel.mjs";

const step = (id, kind) => ({ id, name: id, kind });
const upstream = [step("draft", "agent"), step("ask", "human"), step("tests", "check")];
const inputs = [{ name: "size", label: "Size", kind: "text" }];
const KINDS = CONDITIONS.map((c) => c.condition);

test("a fresh condition names what exists — a judged step, else any step, else an input, else a group to fill — and never a blank reference", () => {
  assert.deepEqual(freshCondition(upstream, inputs), { condition: "outcome", step: "tests", passed: true });
  assert.deepEqual(freshCondition([step("draft", "agent")], inputs), { condition: "outcome", step: "draft", passed: true });
  assert.deepEqual(freshCondition([], inputs), { condition: "input_equals", input: "size", value: "" });
  assert.deepEqual(freshCondition([], []), { condition: "all", of: [] });
  assert.deepEqual(freshCondition(null, undefined), { condition: "all", of: [] });
});

test("a kind that names a step or an input is offered only when there is one, and a group only above the bound — what a condition already is stays listed", () => {
  assert.deepEqual(KINDS.filter((k) => !offeredWith(k, [], [])).sort(), ["answered", "input_equals", "outcome", "output_equals", "output_matches"]);
  assert.deepEqual(KINDS.filter((k) => !offeredWith(k, upstream, [])), ["input_equals"]);
  assert.deepEqual(KINDS.filter((k) => !offeredWith(k, upstream, inputs)), []);
  const at = (depth, current, up = upstream, ins = inputs) => offeredKinds(CONDITIONS, current, depth, up, ins).map((c) => c.condition);
  assert.deepEqual(at(1, "outcome"), KINDS);
  assert.equal(mayNest(MAX_CONDITION_DEPTH - 1), true);
  assert.equal(mayNest(MAX_CONDITION_DEPTH), false);
  assert.deepEqual(at(MAX_CONDITION_DEPTH, "outcome").filter((k) => COMBINATORS.includes(k)), [], "no group is offered at the bound");
  assert.ok(at(1, "input_equals", upstream, []).includes("input_equals"), "a stored condition whose input is gone is shown as it is");
  assert.ok(!at(1, "outcome", upstream, []).includes("input_equals"));
});

test("a change of kind starts a leaf from what exists, and a group from what was there", () => {
  const was = { condition: "outcome", step: "tests", passed: false };
  assert.deepEqual(conditionOf("answered", upstream, inputs, was), { condition: "answered", step: "ask", option: "" });
  assert.deepEqual(conditionOf("output_equals", upstream, inputs, was), { condition: "output_equals", step: "draft", path: "", value: "" });
  assert.deepEqual(conditionOf("output_matches", upstream, inputs, was), { condition: "output_matches", step: "draft", path: "", contains: "" });
  assert.deepEqual(conditionOf("input_equals", upstream, inputs, was), { condition: "input_equals", input: "size", value: "" });
  assert.deepEqual(conditionOf("between", upstream, inputs, was), { condition: "between", from_hour: 9, to_hour: 18 });
  // Wrapping keeps the condition; another group keeps its children; a `not` hands over what it held.
  assert.deepEqual(conditionOf("all", upstream, inputs, was), { condition: "all", of: [was] });
  assert.deepEqual(conditionOf("any", upstream, inputs, { condition: "all", of: [was, was] }), { condition: "any", of: [was, was] });
  assert.deepEqual(conditionOf("one", upstream, inputs, { condition: "not", of: was }), { condition: "one", of: [was] });
  assert.deepEqual(conditionOf("not", upstream, inputs, was), { condition: "not", of: was });
  assert.deepEqual(conditionOf("not", upstream, inputs, { condition: "not", of: was }), { condition: "not", of: was }, "never a not of a not by a change of kind");
  // With nothing to name, a kind that names something falls back to what can be drawn: no reference is ever `""`.
  for (const kind of ["input_equals", "output_equals", "output_matches", "answered", "outcome"]) {
    const made = conditionOf(kind, [], [], was);
    assert.deepEqual(made, { condition: "all", of: [] }, kind);
  }
  for (const kind of KINDS) {
    const made = conditionOf(kind, upstream, inputs, was);
    assert.equal(made.condition, kind);
    for (const key of ["step", "input"]) if (key in made) assert.ok(made[key], `${kind}.${key} names something`);
  }
  assert.equal(isGroup({ condition: "one", of: [] }), true);
  assert.equal(isGroup({ condition: "not", of: was }), false);
  assert.equal(isGroup(null), false);
});

test("a value is typed as JSON when it parses, and what its field shows is what would be written back", () => {
  assert.deepEqual([parseValue("5"), parseValue("true"), parseValue("null"), parseValue("[1, 2]"), parseValue("big"), parseValue(""), parseValue('"5"')], [5, true, null, [1, 2], "big", "", "5"]);
  assert.deepEqual([valueText(5), valueText(true), valueText(null), valueText([1, 2]), valueText("big"), valueText("")], ["5", "true", "null", "[1,2]", "big", ""]);
  // The text `5` is not the number 5: shown bare, the next keystroke would make it one.
  assert.equal(valueText("5"), '"5"');
  assert.equal(valueText("true"), '"true"');
  assert.equal(valueText("[1]"), '"[1]"');
  assert.equal(valueText('say "hi"'), 'say "hi"', "text that reads as nothing else is shown as it is");
  for (const value of [5, 0, -1.5, true, false, null, [1, "two"], { a: 1 }, "big", "", "5", "true", "null", "[1]", '"quoted"', "a b", "{}"]) {
    assert.deepEqual(parseValue(valueText(value)), value, `${JSON.stringify(value)} comes back as it was`);
  }
});

test("an hour is whole and within the day", () => {
  assert.deepEqual(["9", "23", "24", "-1", "7.9", "", "noon"].map(hourOf), [9, 23, 23, 0, 7, 0, 0]);
});

test("the editor holds no rule of its own", () => {
  const editor = readFileSync(new URL("./ConditionEditor.tsx", import.meta.url), "utf8");
  for (const call of ["conditionOf(", "freshCondition(upstream, inputs)", "offeredKinds(CONDITIONS, value.condition, depth, upstream, inputs)", "parseValue(", "valueText(", "hourOf("]) assert.ok(editor.includes(call), call);
  assert.ok(!editor.includes("JSON.parse") && !editor.includes("function blankFor") && !editor.includes("Math.trunc"));
});

// added by the coverage pass: conditionModel.test.mjs
test("a kind the editor does not know keeps what was there, or starts fresh", () => {
  const upstream = [step("ask", "human")];
  const inputs = [];
  const was = { condition: "between", from_hour: 1, to_hour: 2 };
  assert.deepEqual(conditionOf("weird", upstream, inputs, was), was);
  assert.deepEqual(conditionOf("weird", upstream, inputs, undefined), freshCondition(upstream, inputs));
});
