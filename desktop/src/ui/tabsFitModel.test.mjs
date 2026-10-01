/**
 * A tab strip folds to its glyphs stage by stage when its labels would not
 * fit. Run with `node --test desktop/src/ui/tabsFitModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { fitStage, foldGroups, foldedIds, glyphTitle, stripKey } from "./tabsFitModel.mjs";

const glyph = (id, fold) => ({ id, label: id, icon: true, ...(fold === undefined ? {} : { fold }) });

test("the tabs yield their words group by group: ranks in order, the undeclared first and together, a tab with no glyph never", () => {
  assert.deepEqual(foldGroups([glyph("a"), glyph("b"), glyph("c")]), [["a", "b", "c"]], "no order declared: one group, all at once");
  assert.deepEqual(foldGroups([glyph("workspace", 3), glyph("goals", 2), glyph("workflows", 1)]), [["workflows"], ["goals"], ["workspace"]], "ranks ascending, whatever the strip's order");
  assert.deepEqual(foldGroups([glyph("a", 2), glyph("b"), glyph("c", 2), glyph("d", 1)]), [["b"], ["d"], ["a", "c"]], "undeclared first; equal ranks share a group in strip order");
  assert.deepEqual(foldGroups([glyph("a"), { id: "plain", label: "Plain" }]), [["a"]], "no glyph, nothing to fold to");
  assert.deepEqual(foldGroups([{ id: "x", label: "X" }]), [], "a strip nobody gave glyphs to never folds");
  assert.deepEqual(foldGroups([]), []);
});

test("the stage drawn is the first that fits, an unmeasured one being drawn to be measured, the last when none fits", () => {
  assert.equal(fitStage([], 352, 1), 0, "nothing measured: every word, and measure");
  assert.equal(fitStage([300], 352, 1), 0);
  assert.equal(fitStage([352], 352, 1), 0, "exactly fitting is fitting");
  assert.equal(fitStage([420], 352, 1), 1, "too tight and the next stage unmeasured: draw it");
  assert.equal(fitStage([420, 300], 352, 1), 1);
  assert.equal(fitStage([420, 360], 340, 3), 2, "still too tight: the next");
  assert.equal(fitStage([420, 360, 300], 340, 3), 2);
  assert.equal(fitStage([420, 360, 300, 250], 200, 3), 3, "the last stage even when it does not fit — nothing is hidden");
  assert.equal(fitStage([420, null, 300], 340, 3), 1, "a stage forgotten is measured again");
});

test("room grows: a direct jump back over the remembered widths, the words returning in reverse", () => {
  const needed = [420, 360, 300, 250];
  assert.equal(fitStage(needed, 310, 3), 2);
  assert.equal(fitStage(needed, 365, 3), 1);
  assert.equal(fitStage(needed, 420, 3), 0, "the first tab folded is the last to come back");
});

test("a strip whose container is not measured, or that has no stages, draws every word", () => {
  assert.equal(fitStage([420], null, 1), 0);
  assert.equal(fitStage([420], undefined, 1), 0);
  assert.equal(fitStage([420], Number.NaN, 1), 0);
  assert.equal(fitStage([420, 300], 100, 0), 0, "no glyphs, no stage");
});

test("the folded ids are the first groups", () => {
  const groups = [["workflows"], ["goals"], ["workspace"]];
  assert.deepEqual([...foldedIds(groups, 0)], []);
  assert.deepEqual([...foldedIds(groups, 1)], ["workflows"]);
  assert.deepEqual([...foldedIds(groups, 2)], ["workflows", "goals"]);
  assert.deepEqual([...foldedIds(groups, 3)], ["workflows", "goals", "workspace"]);
  assert.deepEqual([...foldedIds([["a", "b"], ["c"]], 1)], ["a", "b"], "a group yields together");
});

test("the measured widths are keyed by what changes a width — a count, a label, a glyph, a rank", () => {
  const tabs = [{ id: "goals", label: "Goals", icon: true, fold: 2, count: 3 }];
  const key = stripKey(tabs);
  assert.equal(stripKey([{ ...tabs[0] }]), key, "the same tab is the same key");
  assert.notEqual(stripKey([{ ...tabs[0], count: 4 }]), key, "a badge grows with its count");
  assert.notEqual(stripKey([{ ...tabs[0], label: "Goal" }]), key);
  assert.notEqual(stripKey([{ ...tabs[0], icon: undefined }]), key);
  assert.notEqual(stripKey([{ ...tabs[0], fold: 1 }]), key);
  assert.notEqual(stripKey([...tabs, { id: "x", label: "X" }]), key, "a tab added");
});

test("a folded tab's tooltip is its label, with its count when it has one", () => {
  assert.equal(glyphTitle("Goals", 3), "Goals · 3");
  assert.equal(glyphTitle("Goals", 0), "Goals");
  assert.equal(glyphTitle("Goals", null), "Goals");
  assert.equal(glyphTitle("Goals"), "Goals");
});
