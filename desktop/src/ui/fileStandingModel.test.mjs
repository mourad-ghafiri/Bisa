/**
 * What the explorer wears for a changed path. Run with
 * `node --test desktop/src/ui/fileStandingModel.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";

import { STANDING_KINDS, foldStandings, standingHint, standingMark, standingTone } from "./fileStandingModel.mjs";

test("every changed file wears its kind, and every folder above it holds the strongest kind beneath", () => {
  const folded = foldStandings([
    { path: "src/app/main.rs", kind: "modified" },
    { path: "src/app/new.rs", kind: "untracked" },
    { path: "src/lib/merge.rs", kind: "conflict" },
    { path: "README.md", kind: "added", staged: true },
  ]);
  assert.deepEqual(folded.get("src/app/main.rs"), { kind: "modified", staged: false, strongest: "modified" });
  assert.deepEqual(folded.get("README.md"), { kind: "added", staged: true, strongest: "added" });
  assert.deepEqual(folded.get("src/app"), { kind: "holds", staged: false, strongest: "modified" }, "a folder holds the strongest kind under it: modified outranks untracked");
  assert.deepEqual(folded.get("src/lib"), { kind: "holds", staged: false, strongest: "conflict" });
  assert.deepEqual(folded.get("src"), { kind: "holds", staged: false, strongest: "conflict" }, "a conflict three levels down is the top folder's colour");
  assert.equal(folded.get("docs"), undefined, "a folder with nothing under it wears nothing");
  assert.equal(foldStandings([]).size, 0);
  assert.equal(foldStandings(null).size, 0);
});

test("the tones are the theme's three status roles — new is ok, moved is warn, a conflict is danger — and a folder wears its strongest", () => {
  assert.deepEqual([...STANDING_KINDS], ["conflict", "deleted", "renamed", "modified", "added", "untracked"], "strongest first");
  assert.equal(standingTone({ kind: "added", staged: false, strongest: "added" }), "ok");
  assert.equal(standingTone({ kind: "untracked", staged: false, strongest: "untracked" }), "ok");
  assert.equal(standingTone({ kind: "modified", staged: false, strongest: "modified" }), "warn");
  assert.equal(standingTone({ kind: "renamed", staged: true, strongest: "renamed" }), "warn");
  assert.equal(standingTone({ kind: "deleted", staged: true, strongest: "deleted" }), "warn");
  assert.equal(standingTone({ kind: "conflict", staged: false, strongest: "conflict" }), "danger");
  assert.equal(standingTone({ kind: "holds", staged: false, strongest: "added" }), "ok");
  assert.equal(standingTone(null), null);
});

test("a file wears the Changes view's mark, a folder none, and the tooltip says the word and the side", () => {
  assert.equal(standingMark({ kind: "modified", staged: false, strongest: "modified" }), "~");
  assert.equal(standingMark({ kind: "added", staged: true, strongest: "added" }), "+");
  assert.equal(standingMark({ kind: "untracked", staged: false, strongest: "untracked" }), "?");
  assert.equal(standingMark({ kind: "conflict", staged: false, strongest: "conflict" }), "!");
  assert.equal(standingMark({ kind: "deleted", staged: true, strongest: "deleted" }), "−");
  assert.equal(standingMark({ kind: "renamed", staged: true, strongest: "renamed" }), "→");
  assert.equal(standingMark({ kind: "holds", staged: false, strongest: "modified" }), null, "a folder's colour says enough");
  assert.equal(standingHint({ kind: "modified", staged: true, strongest: "modified" }), "modified · staged");
  assert.equal(standingHint({ kind: "untracked", staged: false, strongest: "untracked" }), "untracked");
  assert.equal(standingHint({ kind: "holds", staged: false, strongest: "conflict" }), "holds changes");
  assert.equal(standingHint(null), null);
});
