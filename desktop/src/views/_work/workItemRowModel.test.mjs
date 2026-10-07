/**
 * The Work row's line. Run with `node --test desktop/src/views/_work/workItemRowModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { NO_INSTRUCTIONS, headline, movesItem } from "./workItemRowModel.mjs";

test("the first line is the item's name, its heading mark gone", () => {
  assert.equal(headline("# Review the pull request\n\nRead every hunk."), "Review the pull request");
  assert.equal(headline("### Fix the flaky test"), "Fix the flaky test");
});

test("a bullet, a numbered item or a quote loses its mark too", () => {
  assert.equal(headline("- Run the suite"), "Run the suite");
  assert.equal(headline("* Run the suite"), "Run the suite");
  assert.equal(headline("1. Run the suite"), "Run the suite");
  assert.equal(headline("> Run the suite"), "Run the suite");
});

test("blank leading lines are skipped and whitespace collapses", () => {
  assert.equal(headline("\n\n   \n  Write   the\tchangelog  "), "Write the changelog");
  assert.equal(headline("\r\n# Windows line endings\r\nbody"), "Windows line endings");
});

test("only the first line is read — a second paragraph never leaks in", () => {
  assert.equal(headline("Add the endpoint\nThen document it"), "Add the endpoint");
});

test("inline code stays as written", () => {
  assert.equal(headline("Rename `foo()` to `bar()`"), "Rename `foo()` to `bar()`");
});

test("nothing to say is said in words", () => {
  assert.equal(headline(""), NO_INSTRUCTIONS);
  assert.equal(headline("   \n\n"), NO_INSTRUCTIONS);
  assert.equal(headline(null), NO_INSTRUCTIONS);
  assert.equal(headline(undefined), NO_INSTRUCTIONS);
});

test("nothing is cut — the row truncates with CSS", () => {
  const long = "Implement " + "a very long instruction ".repeat(20).trim();
  assert.equal(headline(long), long);
});

test("a panel open on a work item reads it again on the frames that move it, by the item they name", () => {
  assert.equal(movesItem({ payload: { type: "execution_ended", work_item: "w1" } }, "w1"), true);
  assert.equal(movesItem({ payload: { type: "execution_ended", work_item: "w2" } }, "w1"), false, "another item's frame");
  assert.equal(movesItem({ payload: { type: "session_state", presence: { work_item: "w1" } } }, "w1"), true, "its session moved");
  assert.equal(movesItem({ payload: { type: "session_state", presence: { work_item: null } } }, "w1"), false);
  assert.equal(movesItem({ payload: { type: "goal_closed" } }, "w1"), false, "a frame that names no item moves none");
  assert.equal(movesItem(null, "w1"), false);
});
