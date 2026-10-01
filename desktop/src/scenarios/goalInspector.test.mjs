/**
 * The goal's Details pane › Work: dense rows, never framed cards — the state,
 * the instructions' first line, the step; who was asked and who runs it on a
 * second line only when there is someone to name. Source guards, no DOM.
 *
 * Run with `node --test desktop/src/scenarios/goalInspector.test.mjs`.
 */
import { readFileSync } from "node:fs";
import { test } from "node:test";
import assert from "node:assert/strict";

test("the Work list is dense rows whose line is the model's headline", () => {
  const src = readFileSync(new URL("../views/_work/GoalInspector.tsx", import.meta.url), "utf8");
  assert.ok(src.includes('from "./workItemRowModel.mjs"'), "the row's words are the model's");
  const start = src.indexOf("function WorkPanel(");
  const end = src.indexOf("/** The Details panel's project card");
  assert.ok(start > 0 && end > start, "the Work panel is where it was");
  const panel = src.slice(start, end);
  assert.ok(panel.includes('<ul className="flex flex-col gap-0.5">'), "rows sit tight");
  assert.ok(panel.includes("headline(w.instructions)"), "the first line names the item");
  assert.equal((panel.match(/<WorkItemStateChip/g) ?? []).length, 1, "one state chip per row");
  assert.ok(!panel.includes("border border-border"), "no frame around a row");
  assert.ok(!panel.includes("line-clamp"), "no clamped body — the panel has it");
  assert.ok(!/\bp-2\b/.test(panel), "no card padding");
  assert.ok(panel.includes("hover:bg-surface-2"), "the row highlights as the house rows do");
});
