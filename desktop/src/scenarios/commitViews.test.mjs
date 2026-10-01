/**
 * A commit reads in the three views a changed file does (ide/05 §Click to
 * inspect), as the sources show it: the commit document wears the one View
 * control, its file list is a selector, one file of a commit is served by
 * the node in the two shapes the views read, and the comparison is one
 * component both documents draw. A source assertion, as `sidebar.test.mjs`
 * makes them — no DOM.
 *
 * Run with `node --test desktop/src/scenarios/commitViews.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const src = (name) => readFileSync(new URL(name, import.meta.url), "utf8");

test("the commit document reads a commit's file in the three views, with the one choice every patch shares", () => {
  const doc = src("../views/_work/CommitDocument.tsx");
  for (const fact of ['usePanelView("patch")', 'setPanelView("patch"', "PATCH_VIEWS.map(", 'from "./Comparison"', "gitCommitFileDiff(", "gitCommitFileSides(", "patchViewHint(v, { readOnly: true })", "focusOf(", "chooseFile(", 'role="listbox"', "aria-selected={selected}"]) {
    assert.ok(doc.includes(fact), `the commit document ${fact}`);
  }
  assert.ok(!doc.includes("<ReadOnlyDiff diff={d.diff} />"), "the whole patch is one branch of the focus, not the only thing drawn");
  const patch = src("../views/_work/PatchDocument.tsx");
  assert.ok(!patch.includes("function Comparison"), "the comparison left the patch document");
  assert.ok(patch.includes('import { Comparison } from "./Comparison";'), "and is imported from its own file");
  assert.ok(src("../views/_work/Comparison.tsx").includes("sides: PatchSides"), "one shape for both documents' sides");
  const views = src("../views/_workbench/rightPanelModel.mjs");
  assert.ok(views.includes("patch: PATCH_VIEWS") && !views.includes("commit:"), "one remembered choice for every patch, no second key for a commit");
});

test("one file of a commit is served in the two shapes the views read, renames detected end to end", () => {
  const node = src("../../../crates/bisa-node/src/projects.rs");
  for (const fact of ['"/workstreams/{wid}/git/commit/{sha}/diff"', '"/workstreams/{wid}/git/commit/{sha}/sides"', "struct CommitPathQuery", "Json<CommitFileDiff>", "Json<CommitFileSides>"]) {
    assert.ok(node.includes(fact), `the node ${fact}`);
  }
  const schema = src("../../../crates/bisa-node/src/bin/api-schema.rs");
  assert.ok(schema.includes('"CommitFileDiff"') && schema.includes('"CommitFileSides"'), "both shapes reach types.gen.ts");
  const engine = src("../../../crates/bisa-engine/src/ide/git.rs");
  for (const fact of ["pub fn commit_file_plan(", "pub async fn commit_file(", "pub async fn commit_file_sides(", "BlobRev::Rev("]) {
    assert.ok(engine.includes(fact), `the engine ${fact}`);
  }
  const vcs = src("../../../crates/bisa-vcs/src/git.rs");
  assert.ok(vcs.includes("Rev(String),"), "a blob is read at any one revision");
  assert.ok(vcs.includes("pub fn commit_diff(&self, path: &Path, commit: &str, paths: &[&str])"), "a commit's patch is asked by file");
  assert.equal((vcs.match(/s\("-M"\),/g) ?? []).length, 2, "the detail and the patch detect renames in lockstep");
});
