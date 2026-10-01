/**
 * Exact matches on an event's fields — and an emit's payload — as rows a
 * person edits: a path renamed in place, never onto another row; a fresh
 * path no row has; a removal; a value. Run with
 * `node --test --import ./src/i18n/preload.mjs desktop/src/views/_workflow/forms/exactFieldsModel.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";

import { MAX_PATH, addField, fieldRows, freshPath, removeField, renameField, setFieldValue } from "./exactFieldsModel.mjs";

const fields = Object.freeze({ env: "prod", region: "{inputs.region}", tier: "gold" });

test("the rows are the map's entries, in the order they were written", () => {
  assert.deepEqual(fieldRows(fields), [
    { path: "env", value: "prod" },
    { path: "region", value: "{inputs.region}" },
    { path: "tier", value: "gold" },
  ]);
  assert.deepEqual(fieldRows(null), []);
  assert.deepEqual(fieldRows(undefined), []);
});

test("a path is renamed where it stands: the row keeps its place and its value, and the others are untouched", () => {
  const r = renameField(fields, "env", "environment");
  assert.equal(r.ok, true);
  assert.deepEqual(Object.keys(r.fields), ["environment", "region", "tier"], "the first row is still the first");
  assert.equal(r.fields.environment, "prod");
  assert.deepEqual(fields, { env: "prod", region: "{inputs.region}", tier: "gold" }, "the map handed in is never written to");
  assert.deepEqual(renameField(fields, "region", " payload.region ").fields, { env: "prod", "payload.region": "{inputs.region}", tier: "gold" }, "trimmed; a dotted path reads into the payload");
  const same = renameField(fields, "env", "env");
  assert.equal(same.ok, true);
  assert.equal(same.fields, fields, "the same name is no edit");
});

test("a rename never lands on another row: the other field would be lost", () => {
  const taken = renameField(fields, "env", "tier");
  assert.deepEqual(taken, { ok: false, reason: "Another field already matches `tier`." });
  assert.deepEqual(renameField(fields, "env", "  "), { ok: false, reason: "A field needs a path." });
  assert.deepEqual(renameField(fields, "gone", "x"), { ok: false, reason: "There is no field `gone`." });
  assert.equal(renameField(fields, "env", "x".repeat(MAX_PATH)).ok, true, "as long as a path may be");
  assert.deepEqual(renameField(fields, "env", "x".repeat(MAX_PATH + 1)), { ok: false, reason: `A path is at most ${MAX_PATH} characters.` }, "one past it");
});

test("a new field takes a path no row has — a number that was freed is never taken from a row that still has it", () => {
  assert.equal(freshPath({}), "field");
  assert.equal(freshPath({ field: "" }), "field-2");
  assert.equal(freshPath({ field: "a", "field-2": "b" }), "field-3");
  // `field` was removed, `field-2` stays: the next is `field`, and `field-2` keeps its value.
  const holed = { "field-2": "kept" };
  assert.equal(freshPath(holed), "field");
  assert.deepEqual(addField(holed), { "field-2": "kept", field: "" });
  assert.deepEqual(addField(addField({})), { field: "", "field-2": "" });
  assert.deepEqual(addField(null), { field: "" });
});

test("a removal takes one row; a value is written on its row alone", () => {
  assert.deepEqual(removeField(fields, "region"), { env: "prod", tier: "gold" });
  assert.equal(removeField(fields, "gone"), fields, "nothing to remove: the same map");
  assert.deepEqual(setFieldValue(fields, "tier", "silver"), { env: "prod", region: "{inputs.region}", tier: "silver" });
  assert.deepEqual(Object.keys(setFieldValue(fields, "env", "staging")), ["env", "region", "tier"], "its place is kept");
  assert.equal(setFieldValue(fields, "gone", "x"), fields, "a value for no row writes nothing");
});
