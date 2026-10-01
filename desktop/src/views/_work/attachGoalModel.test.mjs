/**
 * The attach dialog's decisions, tested where they live — and the one rule
 * about where attaching happens: one dialog, opened from About and the rail,
 * never from the creation dialog. Nothing here renders (no jsdom in this
 * repo); the guard reads the sources, as `deadExports.test.mjs` does.
 *
 * Run with `npm test` from `desktop/`.
 */
import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { sourceFiles } from "../../testWalk.mjs";
import { attachChoice, attachEmptyWords, attachableGoals, attachedWords, goalLabelOf } from "./attachGoalModel.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const SRC = join(HERE, "..", "..");
const G = (id, label) => ({ id, label });
const GOALS = [G("01A", "Dark mode"), G("01B", "Checkout"), G("01C", "Docs")];

test("the goals offered are the ones the project is not yet on, in the workspace's order", () => {
  assert.deepEqual(attachableGoals(GOALS, []), GOALS);
  assert.deepEqual(attachableGoals(GOALS, ["01B"]), [GOALS[0], GOALS[2]]);
  assert.deepEqual(attachableGoals(GOALS, ["01A", "01B", "01C"]), [], "every goal has it: nothing to offer");
  assert.deepEqual(attachableGoals(GOALS, null), GOALS, "a row with no goals yet is on none");
  assert.deepEqual(attachableGoals([], ["01A"]), [], "no goals at all");
});

test("the only candidate is chosen for you; several are an explicit pick; a pick stands while it is offered and gives way when it is gone", () => {
  assert.equal(attachChoice([GOALS[1]]), "01B", "one goal: nothing to decide");
  assert.equal(attachChoice(GOALS), "", "several: the person chooses — attaching is one click with no confirmation after it");
  assert.equal(attachChoice([]), "", "nothing to offer, nothing chosen");
  assert.equal(attachChoice(GOALS, "01C"), "01C", "the pick stands");
  assert.equal(attachChoice([GOALS[0]], "01C"), "01A", "a pick no longer offered gives way to the only candidate");
  assert.equal(attachChoice([GOALS[0], GOALS[1]], "01C"), "", "…or to nothing, when there are several");
  assert.equal(attachChoice(GOALS, null), "", "no pick yet");
});

test("nothing to offer says why: no goal yet, or every goal already has the project", () => {
  assert.equal(attachEmptyWords(0), "There are no goals yet. Capture one first, then attach this project to it.");
  assert.equal(attachEmptyWords(3), "Every goal already has this project attached.");
});

test("the toast names both ends, and a goal the list no longer has is named by its tail", () => {
  // The guide's own example: *web-app is attached to Dark mode.*
  assert.equal(attachedWords("web-app", "Dark mode"), "web-app is attached to Dark mode.");
  assert.equal(goalLabelOf(GOALS, "01B"), "Checkout");
  assert.equal(goalLabelOf(GOALS, "01HGONE99"), "GONE99", "the list moved on: the id's tail, never nothing");
});

test("attaching is one dialog: About and the rail render it, creation does not, and the record is written from two places only — the dialog and a capture's hand-over", () => {
  const read = (rel) => readFileSync(join(SRC, rel), "utf8");
  for (const rel of ["views/_work/ProjectDetail.tsx", "views/_workbench/ProjectRail.tsx"]) {
    assert.ok(read(rel).includes("<AttachGoalDialog"), `${rel} opens the one attach dialog`);
  }
  const creation = read("views/_work/NewProjectDialog.tsx");
  assert.ok(!creation.includes("<AttachGoalDialog") && !creation.includes("api.attachProject"), "creating a project attaches nothing by hand: the route does, for the goal a door fixed");
  const isSource = (p) => /\.(ts|tsx|mjs)$/.test(p) && !p.endsWith(".d.mts") && !/\.test\.mjs$/.test(p);
  const callers = sourceFiles(SRC, isSource)
    .filter((p) => readFileSync(p, "utf8").includes("api.attachProject("))
    .map((p) => relative(SRC, p))
    .sort();
  assert.deepEqual(callers, ["views/_work/AttachGoalDialog.tsx", "views/_work/NewGoalDialog.tsx"], "a third door to attaching is a door to read first");
});
