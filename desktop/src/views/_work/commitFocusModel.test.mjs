/**
 * What a commit document reads under its file list. Run with
 * `node --test desktop/src/views/_work/commitFocusModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { chooseFile, cutWords, emptyPatchWords, focusOf, isFocused, listHint } from "./commitFocusModel.mjs";
import { PATCH_VIEWS } from "./patchViewModel.mjs";

const files = [
  { path: "a.txt", old_path: null, kind: "modified" },
  { path: "docs/README.md", old_path: "README.md", kind: "renamed" },
  { path: "gone.txt", old_path: null, kind: "deleted" },
];

test("on Hunks the whole patch is read until a file is chosen, and the chosen file alone after", () => {
  assert.deepEqual(focusOf(files, null, "hunks"), { kind: "all" });
  assert.deepEqual(focusOf(files, "a.txt", "hunks"), { kind: "file", path: "a.txt", oldPath: null, change: "modified" });
  assert.deepEqual(focusOf(files, "docs/README.md", "hunks"), { kind: "file", path: "docs/README.md", oldPath: "README.md", change: "renamed" }, "a rename carries where the file was");
  assert.deepEqual(focusOf(files, "elsewhere.txt", "hunks"), { kind: "all" }, "a choice the list no longer holds falls back to the whole patch");
  assert.deepEqual(focusOf([], null, "hunks"), { kind: "all" }, "a commit that touched no file still has its (empty) patch");
});

test("a comparison reads the chosen file, else the first, and nothing when the commit touched none", () => {
  for (const view of ["split", "inline"]) {
    assert.deepEqual(focusOf(files, null, view), { kind: "file", path: "a.txt", oldPath: null, change: "modified" }, `${view} opens on the first file`);
    assert.deepEqual(focusOf(files, "gone.txt", view), { kind: "file", path: "gone.txt", oldPath: null, change: "deleted" });
    assert.deepEqual(focusOf(files, "elsewhere.txt", view), { kind: "file", path: "a.txt", oldPath: null, change: "modified" }, "a stale choice falls back to the first file");
    assert.equal(focusOf([], null, view), null, "nothing to compare");
  }
});

test("a click chooses the row; on Hunks a second click on the chosen row lets go, a comparison keeps its file", () => {
  assert.equal(chooseFile(null, "a.txt", "hunks"), "a.txt");
  assert.equal(chooseFile("a.txt", "gone.txt", "hunks"), "gone.txt");
  assert.equal(chooseFile("a.txt", "a.txt", "hunks"), null, "letting go reads the whole patch again");
  assert.equal(chooseFile("a.txt", "a.txt", "split"), "a.txt", "a comparison always reads one file");
  assert.equal(chooseFile("a.txt", "a.txt", "inline"), "a.txt");
  assert.ok(isFocused(focusOf(files, null, "split"), "a.txt"), "the row a comparison fell back to reads as in hand");
  assert.ok(!isFocused(focusOf(files, null, "hunks"), "a.txt"), "on the whole patch no row is in hand");
  assert.ok(!isFocused(null, "a.txt"));
});

test("the sentence over an empty patch names a move, a copy, a lineless change or a commit without text; the list's hint follows the view", () => {
  assert.match(emptyPatchWords({ kind: "all" }), /No textual change against the first parent/);
  assert.match(emptyPatchWords(null), /No textual change/);
  assert.match(emptyPatchWords({ kind: "file", path: "docs/README.md", oldPath: "README.md", change: "renamed" }), /^Renamed without changes/);
  assert.match(emptyPatchWords({ kind: "file", path: "b.txt", oldPath: "a.txt", change: "copied" }), /^Copied without changes/);
  assert.match(emptyPatchWords({ kind: "file", path: "x", oldPath: null, change: "modified" }), /No lines changed/);
  for (const view of PATCH_VIEWS) assert.ok(listHint(view).startsWith("Pick a file"), `${view} says what a click does`);
  assert.match(listHint("hunks"), /whole patch/);
  assert.match(listHint("split"), /compare/);
});

test("a patch the node cut says so without a number of the desktop's: the bound is the node's", () => {
  assert.equal(cutWords(), "The patch was cut where the read stops; the file list above is complete.");
  assert.ok(!/MiB|MB/.test(cutWords()));
});
