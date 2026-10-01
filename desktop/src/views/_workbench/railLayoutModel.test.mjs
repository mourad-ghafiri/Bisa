import test from "node:test";
import assert from "node:assert/strict";
import { INDENT_BASE_PX, INDENT_PX, TWISTY_PX, guideInset, rowHeightToken } from "./railLayoutModel.mjs";

test("one level is one disclosure column, and the guide hangs from the chevron's centre", () => {
  assert.equal(INDENT_PX, TWISTY_PX, "a child's row starts where its parent's content does");
  for (const depth of [0, 1, 2, 3, 4]) {
    // The guide of the level below a row sits under that row's chevron.
  }
  assert.equal(guideInset(), TWISTY_PX / 2);
});

test("a project is the one tall card; every other row is a tree row", () => {
  assert.equal(rowHeightToken("project"), "--spacing-row");
  for (const kind of ["group", "goal", "workstream", "terminal", "agent"]) assert.equal(rowHeightToken(kind), "--spacing-row-sm", kind);
});
