/**
 * The three counts the pet stands by and an addon is told, tested where the
 * rule lives. No DOM.
 *
 * Run with `npm test` from `desktop/`.
 */
import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { workSummary } from "./workSummaryModel.mjs";
import { standingState } from "../pet/petModel.mjs";

const row = (...kinds) => ({ needs_action: kinds.map((gate_kind) => ({ gate_kind })) });

test("what waits, what is open for review and what is worked on are counted from the shell's lists and the roster's tally", () => {
  const ws = { inbox: [row("approval"), row("question"), row("approval", "question"), row()], waiting: 2, working: { c1: ["scout"], c2: [], c3: ["ada", "scout"] } };
  assert.deepEqual(workSummary(ws, { waiting: 1, working: 3 }), { waiting: 3, review: 2, working: 5, busyScope: "c1" });
  assert.deepEqual(workSummary({ inbox: [], waiting: 0, working: {} }, { waiting: 0, working: 0 }), { waiting: 0, review: 0, working: 0, busyScope: null });
  assert.equal(workSummary({ inbox: [{ needs_action: null }, {}], waiting: 0, working: { c2: [] } }, { waiting: 0, working: 0 }).working, 0, "a conversation nobody is mid-turn in is no work");
});

test("the pet and an addon's summary read the one rule: they never say two things about one moment", () => {
  const busy = workSummary({ inbox: [], waiting: 0, working: { c9: ["scout"] } }, { waiting: 0, working: 0 });
  assert.equal(standingState(busy), "running");
  assert.equal(standingState(workSummary({ inbox: [row("approval")], waiting: 0, working: { c9: ["scout"] } }, { waiting: 0, working: 0 })), "review", "a gate open outranks an agent working");
  assert.equal(standingState(workSummary({ inbox: [row("approval")], waiting: 0, working: {} }, { waiting: 1, working: 0 })), "waiting", "a session waiting on the person outranks both");
  const src = (rel) => readFileSync(new URL(rel, import.meta.url), "utf8");
  for (const reader of ["../pet/PetCompanion.tsx", "../addons/AddonLayer.tsx"]) {
    const text = src(reader);
    assert.ok(text.includes("workSummary(ws, sessionCounts(sessions))"), `${reader} asks the model`);
    assert.ok(!text.includes('gate_kind === "approval"'), `${reader} restates no part of the rule`);
  }
});
