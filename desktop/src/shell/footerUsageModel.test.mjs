/**
 * The footer's usage stat: which harness it shows and its words. Run with
 * `node --test desktop/src/shell/footerUsageModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { pickerRows, pinnedHarness, usageStatWords } from "./footerUsageModel.mjs";

const rows = [
  { id: "claude-code", installed: true },
  { id: "codex", installed: true },
  { id: "omp", installed: false },
];

test("the picker offers the installed harnesses, in the catalog's order", () => {
  assert.deepEqual(
    pickerRows(rows).map((r) => r.id),
    ["claude-code", "codex"],
  );
  assert.deepEqual(pickerRows([]), []);
});

test("the footer shows the pinned harness while it can, else the first offered, else none", () => {
  assert.equal(pinnedHarness("codex", rows), "codex");
  assert.equal(pinnedHarness("omp", rows), "claude-code", "a pinned harness no longer installed falls back");
  assert.equal(pinnedHarness("nope", rows), "claude-code", "a pinned id the catalog does not know falls back");
  assert.equal(pinnedHarness(null, rows), "claude-code");
  assert.equal(pinnedHarness("codex", [{ id: "omp", installed: false }]), null, "nothing installed: nothing shown");
});

test("the stat's words are the harness, its meters with the first reset in place — or its sentence — then the door", () => {
  const meters = [
    { label: "5h", percent: 23 },
    { label: "Weekly", percent: 41 },
    { label: "Fable", percent: 9 },
  ];
  assert.equal(usageStatWords("Claude Code", { meters, inline: "resets in 2 h", note: null }), "Claude Code usage · 5h 23% · resets in 2 h · Weekly 41% · Fable 9% · click for every harness");
  assert.equal(usageStatWords("Claude Code", { meters, inline: null, note: null }), "Claude Code usage · 5h 23% · Weekly 41% · Fable 9% · click for every harness");
  assert.equal(usageStatWords("pi", { meters: [], inline: null, note: "pi reports no usage limits — its provider does." }), "pi usage · pi reports no usage limits — its provider does. · click for every harness");
});
