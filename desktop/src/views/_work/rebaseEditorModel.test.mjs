/**
 * The interactive rebase editor's rules. Run with
 * `node --test desktop/src/views/_work/rebaseEditorModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { REBASE_ACTIONS, REBASE_ACTION_WORDS, editorRows, moveStep, planConsent, planIsIdentity, planOf, planProblem, planSummary, setAction, setMessage } from "./rebaseEditorModel.mjs";

const listed = [
  { id: "c3", short: "c3", subject: "three" },
  { id: "c2", short: "c2", subject: "two" },
  { id: "c1", short: "c1", subject: "one", author: "Ada" },
];

test("the rows are the commits oldest first, every one a pick with its own message; the actions each have a word and a sentence", () => {
  const rows = editorRows(listed);
  assert.deepEqual(
    rows.map((r) => [r.id, r.action, r.message, r.author]),
    [
      ["c1", "pick", "one", "Ada"],
      ["c2", "pick", "two", ""],
      ["c3", "pick", "three", ""],
    ],
  );
  assert.deepEqual([...REBASE_ACTIONS], ["pick", "reword", "squash", "fixup", "drop"]);
  for (const a of REBASE_ACTIONS) assert.ok(REBASE_ACTION_WORDS[a].label && REBASE_ACTION_WORDS[a].meaning.endsWith("."));
  assert.ok(planIsIdentity(rows, listed), "untouched, the plan changes nothing");
});

test("a row moves one place and the ends hold; a squash composes its message from the commit kept above", () => {
  const rows = editorRows(listed);
  assert.deepEqual(
    moveStep(rows, 2, "up").map((r) => r.id),
    ["c1", "c3", "c2"],
  );
  assert.deepEqual(
    moveStep(rows, 0, "up").map((r) => r.id),
    ["c1", "c2", "c3"],
    "the top holds",
  );
  assert.deepEqual(
    moveStep(rows, 2, "down").map((r) => r.id),
    ["c1", "c2", "c3"],
    "the bottom holds",
  );
  assert.ok(!planIsIdentity(moveStep(rows, 2, "up"), listed));
  const squashed = setAction(rows, 1, "squash");
  assert.equal(squashed[1].message, "one\n\ntwo");
  const dropped = setAction(setAction(rows, 0, "drop"), 2, "squash");
  assert.equal(dropped[2].message, "two\n\nthree", "the commit kept above, past a dropped one");
  const reworded = setMessage(setAction(rows, 0, "reword"), 0, "one, reworded");
  assert.equal(reworded[0].message, "one, reworded");
  assert.equal(rows[0].message, "one", "the rows are not mutated");
});

test("the problems are the crate's rules as sentences, and the summary counts what changes", () => {
  const rows = editorRows(listed);
  assert.equal(planProblem(rows), null);
  assert.equal(planProblem([]), "There is nothing to rebase.");
  assert.match(planProblem(rows.map((r) => ({ ...r, action: "drop" }))), /nothing would be left/);
  assert.match(planProblem(setAction(rows, 0, "squash")), /c1 cannot fold into a commit above it/);
  assert.match(planProblem(setAction(setAction(rows, 0, "drop"), 1, "fixup")), /c2 cannot fold/);
  assert.match(planProblem(setMessage(setAction(rows, 1, "reword"), 1, "  ")), /c2 is reworded without a message/);
  assert.equal(planSummary(rows), "3 commits → 3, unchanged");
  const busy = setAction(setAction(setMessage(setAction(rows, 0, "reword"), 0, "x"), 1, "squash"), 2, "drop");
  assert.equal(planSummary(busy), "3 commits → 1: one reworded, one squashed, one dropped");
  assert.equal(planSummary([rows[0]]), "1 commit → 1, unchanged");
});

test("the plan sent is the rows in order with a message only where one is read, and its confirmation names the summary", () => {
  const rows = setAction(setMessage(setAction(editorRows(listed), 0, "reword"), 0, "one, reworded"), 2, "fixup");
  const plan = planOf(rows, "main");
  assert.deepEqual(plan, {
    upstream: "main",
    onto: null,
    steps: [
      { action: "reword", commit: "c1", message: "one, reworded" },
      { action: "pick", commit: "c2", message: null },
      { action: "fixup", commit: "c3", message: null },
    ],
  });
  const squash = planOf(setAction(editorRows(listed), 1, "squash"), "main", "release");
  assert.equal(squash.onto, "release");
  assert.equal(squash.steps[1].message, "one\n\ntwo", "a squash carries the composed message");
  const c = planConsent("topic", "main", "3 commits → 2: one squashed");
  assert.equal(c.title, "Rebase topic interactively onto main?");
  assert.match(c.body, /^3 commits → 2: one squashed\. A conflict/);
  assert.equal(c.confirm, "Rebase");
});
