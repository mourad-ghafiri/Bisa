/**
 * The Changes view's filter: which rows each word keeps.
 * Run with `node --test desktop/src/views/_work/changesFilterModel.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";

import { CHANGE_FILTERS } from "../_workbench/rightPanelModel.mjs";
import { admits, emptyWords, filterCounts, filterGitFiles, filterWords } from "./changesFilterModel.mjs";

const row = (path, index, worktree, flags = {}) => ({
  path,
  index,
  worktree,
  staged: index !== "." && index !== "?" && !flags.conflicted,
  unstaged: worktree !== "." && !flags.conflicted,
  untracked: index === "?",
  conflicted: false,
  ...flags,
});

const stagedOnly = row("a.rs", "M", ".");
const stagedAndEdited = row("b.rs", "M", "M");
const unstagedOnly = row("c.rs", ".", "M");
const added = row("d.rs", "A", ".");
const deleted = row("e.rs", ".", "D");
const renamed = row("f.rs", "R", ".", { old_path: "old.rs" });
const untracked = row("g.rs", "?", "?", { untracked: true, unstaged: true, staged: false });
const conflict = row("h.rs", "U", "U", { conflicted: true, staged: false, unstaged: false });
const FILES = [stagedOnly, stagedAndEdited, unstagedOnly, added, deleted, renamed, untracked, conflict];
const paths = (rows) => rows.map((r) => r.path);

test("each word keeps its own rows, in the order they came", () => {
  assert.deepEqual(paths(filterGitFiles(FILES, "all")), paths(FILES));
  assert.deepEqual(paths(filterGitFiles(FILES, "staged")), ["a.rs", "b.rs", "d.rs", "f.rs"], "the index's side, whatever the letter");
  assert.deepEqual(paths(filterGitFiles(FILES, "unstaged")), ["b.rs", "c.rs", "e.rs"], "the working tree's side of a tracked file");
  assert.deepEqual(paths(filterGitFiles(FILES, "tracked")), ["a.rs", "b.rs", "c.rs", "d.rs", "e.rs", "f.rs", "h.rs"], "everything git knows, a conflict included");
  assert.deepEqual(paths(filterGitFiles(FILES, "untracked")), ["g.rs"]);
  assert.deepEqual(paths(filterGitFiles(FILES, "modified")), ["a.rs", "b.rs", "c.rs"], "the letter M on either side — not added, deleted, renamed or new");
});

test("unstaged and untracked never overlap, though git says both of a new file", () => {
  assert.ok(untracked.unstaged, "the wire's fact");
  assert.ok(!admits(untracked, "unstaged"));
  assert.ok(admits(untracked, "untracked"));
  assert.ok(!admits(untracked, "tracked"));
  for (const r of FILES) assert.ok(!(admits(r, "unstaged") && admits(r, "untracked")), r.path);
});

test("all is the same array, and junk is neither kept nor thrown at", () => {
  assert.equal(filterGitFiles(FILES, "all"), FILES);
  assert.equal(filterGitFiles(FILES, "nonsense"), FILES, "an unknown word narrows nothing");
  assert.deepEqual(filterGitFiles(null, "staged"), []);
  assert.deepEqual(filterGitFiles([null, 7, { index: "M" }], "staged"), []);
  assert.ok(!admits(null, "all"));
});

test("the counts are every word's at once and equal the filtered lengths", () => {
  const counts = filterCounts(FILES);
  assert.deepEqual(Object.keys(counts), [...CHANGE_FILTERS]);
  for (const f of CHANGE_FILTERS) assert.equal(counts[f], filterGitFiles(FILES, f).length, f);
  assert.deepEqual(filterCounts([]), { all: 0, conflicted: 0, staged: 0, unstaged: 0, tracked: 0, untracked: 0, modified: 0 });
  const unmerged = { path: "u.rs", index: "U", worktree: "U", staged: false, unstaged: false, untracked: false, conflicted: true };
  assert.ok(admits(unmerged, "conflicted") && admits(unmerged, "tracked") && !admits(unmerged, "staged"), "an unmerged path is conflicted and tracked, and on no side");
  assert.ok(!admits(FILES[0], "conflicted"));
  assert.equal(filterWords("conflicted", 2), "Conflicted · 2");
  assert.equal(emptyWords("conflicted"), "Nothing conflicted");
});

test("the trigger says the word, with its count unless it is All; an emptied tree names the word", () => {
  assert.equal(filterWords("all", 8), "All");
  assert.equal(filterWords("staged", 3), "Staged · 3");
  assert.equal(filterWords("untracked", 0), "Untracked · 0");
  assert.equal(emptyWords("staged"), "Nothing staged");
  assert.equal(emptyWords("untracked"), "Nothing untracked");
  assert.equal(emptyWords("all"), "Nothing to show");
});
