import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { INDENT_BASE_PX, INDENT_PX, TWISTY_PX, guideInset, guideLeft, rowHeightToken, rowIndent } from "./railLayoutModel.mjs";

test("one level is one disclosure column, and the guide hangs from the chevron's centre", () => {
  assert.equal(INDENT_PX, TWISTY_PX, "a child's row starts where its parent's content does");
  assert.equal(rowIndent(0), INDENT_BASE_PX, "a top-level row starts at the base");
  for (const depth of [1, 2, 3, 4]) {
    assert.equal(rowIndent(depth), rowIndent(depth - 1) + TWISTY_PX, `level ${depth} is one disclosure column deeper`);
    // The guide of level `depth` sits under the chevron of the row one level up.
    assert.equal(guideLeft(depth), rowIndent(depth - 1) + TWISTY_PX / 2, `the guide of level ${depth}`);
  }
  assert.equal(guideInset(), TWISTY_PX / 2);
});

test("the tree's arithmetic and the rail's constants are the model's", () => {
  const tree = readFileSync(new URL("../../ui/tree/TreeList.tsx", import.meta.url), "utf8");
  assert.ok(tree.includes("indent: indentBase + row.depth * indent"), "a row's inset is `rowIndent`");
  assert.ok(tree.includes("left: base + (d - 1) * indent + guideInset"), "a guide's left is `guideLeft`");
  const rail = readFileSync(new URL("./ProjectRail.tsx", import.meta.url), "utf8");
  for (const prop of ["indent={INDENT_PX}", "indentBase={INDENT_BASE_PX}", "guideInset={guideInset()}"]) assert.ok(rail.includes(prop), prop);
});

test("a project is the one tall card; every other row is a tree row", () => {
  assert.equal(rowHeightToken("project"), "--spacing-row");
  for (const kind of ["group", "goal", "workstream", "terminal", "agent"]) assert.equal(rowHeightToken(kind), "--spacing-row-sm", kind);
});
