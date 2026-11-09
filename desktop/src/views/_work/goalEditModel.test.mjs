/**
 * The Details pane's edit of a goal's own record, tested where its rules
 * live. Run with `npm test` from `desktop/` (the script preloads the
 * catalogue).
 */

import { strict as assert } from "node:assert";
import { test } from "node:test";

import { canSave, draftOf, editBody, editable, problemOf } from "./goalEditModel.mjs";

const goal = { id: "G1", title: "The release", statement: "ship it by Friday", tags: ["release"], closed: null };
const live = { id: "R1", started_at: 10, outcome: null, cancelled: null };
const done = { ...live, outcome: "done" };
const stopped = { ...live, cancelled: { cause: "stopped" } };

test("a closed goal takes no edit; a live run fixes the statement alone, and says why", () => {
  assert.deepEqual(editable(goal, null), { can: true, statementLocked: false, why: null });
  assert.equal(editable({ ...goal, closed: { at: 9 } }, null).can, false);
  assert.equal(editable(null, null).can, false);
  const during = editable(goal, live);
  assert.equal(during.can, true, "the title and the tags still move");
  assert.equal(during.statementLocked, true);
  assert.match(during.why, /run/);
  for (const run of [done, stopped]) assert.equal(editable(goal, run).statementLocked, false);
  assert.equal(editable(goal, { id: "Q", started_at: null, outcome: null, cancelled: null }).statementLocked, true, "a queued run counts");
});

test("the form starts from the goal's own words, and an untitled, untagged goal from nothing", () => {
  assert.deepEqual(draftOf(goal), { title: "The release", statement: "ship it by Friday", tags: ["release"] });
  assert.deepEqual(draftOf({ statement: "s" }), { title: "", statement: "s", tags: [] });
  const draft = draftOf(goal);
  draft.tags.push("x");
  assert.deepEqual(goal.tags, ["release"], "the draft is its own list");
});

test("only what changed is sent — trimmed — and nothing when nothing did", () => {
  assert.equal(editBody(goal, draftOf(goal)), null);
  assert.equal(editBody(goal, { ...draftOf(goal), title: "  The release  " }), null, "spaces are no change");
  assert.deepEqual(editBody(goal, { ...draftOf(goal), title: " Release 1.0 " }), { title: "Release 1.0" });
  assert.deepEqual(editBody(goal, { ...draftOf(goal), title: "" }), { title: "" }, "a blank title clears it");
  assert.deepEqual(editBody(goal, { ...draftOf(goal), statement: " ship it by Monday " }), { statement: "ship it by Monday" });
  assert.deepEqual(editBody(goal, { ...draftOf(goal), tags: [] }), { tags: [] }, "an empty list clears the tags");
  assert.deepEqual(editBody(goal, { title: "A", statement: "b", tags: ["release", "ops"] }), { title: "A", statement: "b", tags: ["release", "ops"] });
});

test("a blank statement is refused before it is sent, and Save waits for a change", () => {
  assert.match(problemOf({ title: "t", statement: "   ", tags: [] }), /statement/i);
  assert.equal(problemOf(draftOf(goal)), null);
  assert.equal(canSave(goal, draftOf(goal)), false, "nothing changed");
  assert.equal(canSave(goal, { ...draftOf(goal), statement: " " }), false);
  assert.equal(canSave(goal, { ...draftOf(goal), title: "Renamed" }), true);
});
