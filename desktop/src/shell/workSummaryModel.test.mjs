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
  assert.deepEqual(workSummary(ws, { waiting: 1, working: 3 }), { waiting: 2, review: 2, working: 5, busyScope: "c1" });
  assert.deepEqual(workSummary({ inbox: [], waiting: 0, working: {} }, { waiting: 0, working: 0 }), { waiting: 0, review: 0, working: 0, busyScope: null });
  assert.equal(workSummary({ inbox: [{ needs_action: null }, {}], waiting: 0, working: { c2: [] } }, { waiting: 0, working: 0 }).working, 0, "a conversation nobody is mid-turn in is no work");
});

test("a wait is counted once: the Inbox is the ledger, and a harness at its prompt is already a row of it", () => {
  // The roster says one session waits; the Inbox holds its row. One wait, not two.
  assert.equal(workSummary({ inbox: [], waiting: 1, working: {} }, { waiting: 1, working: 0 }).waiting, 1);
  // The roster's tally alone — a frame the Inbox has not drawn yet — is not a count the pet jumps to.
  assert.equal(workSummary({ inbox: [], waiting: 0, working: {} }, { waiting: 1, working: 0 }).waiting, 0);
});

test("the pet and an addon's summary read the one rule: they never say two things about one moment", () => {
  const busy = workSummary({ inbox: [], waiting: 0, working: { c9: ["scout"] } }, { waiting: 0, working: 0 });
  assert.equal(standingState(busy), "running");
  // With the rows to hand, a turn mid-thought and the hint its frame left are one count, not two.
  const turn = { id: "s1", kind: "conversation", conversation: "c9", state: { state: "running" }, origin: { origin: "turn", scope: "c9" } };
  assert.equal(workSummary({ inbox: [], waiting: 0, working: { c9: ["scout"] } }, { waiting: 0, working: 1 }, [turn]).working, 1, "the row and its scope's hint are one turn");
  assert.equal(workSummary({ inbox: [], waiting: 0, working: { c9: ["scout"] } }, { waiting: 0, working: 1 }).working, 2, "without rows, the old sum stands — a caller of an older shape is not broken");
  assert.equal(standingState(workSummary({ inbox: [row("approval")], waiting: 0, working: { c9: ["scout"] } }, { waiting: 0, working: 0 })), "review", "a gate open outranks an agent working");
  assert.equal(standingState(workSummary({ inbox: [row("approval")], waiting: 1, working: {} }, { waiting: 1, working: 0 })), "waiting", "a session waiting on the person — its Inbox row — outranks both");
  const src = (rel) => readFileSync(new URL(rel, import.meta.url), "utf8");
  for (const reader of ["../pet/PetCompanion.tsx", "../addons/AddonLayer.tsx"]) {
    const text = src(reader);
    assert.ok(text.includes("workSummary(ws, sessionCounts(sessions), sessions)"), `${reader} asks the model, the roster's rows to hand`);
    assert.ok(!text.includes('gate_kind === "approval"'), `${reader} restates no part of the rule`);
  }
});
