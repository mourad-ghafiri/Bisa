/**
 * What a frame means to git, and which view re-reads on it. Run with
 * `node --test desktop/src/views/_work/gitChangeModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { GIT_CHANGES, GIT_COALESCE_MS, gitChangeOf, kindsOf, readsFor } from "./gitChangeModel.mjs";

test("every path the watcher reports has its kind, and a .git path the panel cannot show is none", () => {
  assert.equal(gitChangeOf("src/main.rs"), "worktree");
  assert.equal(gitChangeOf("README.md"), "worktree");
  assert.equal(gitChangeOf(".gitignore"), "worktree");
  assert.equal(gitChangeOf(".git/index"), "index");
  assert.equal(gitChangeOf(".git/HEAD"), "head");
  assert.equal(gitChangeOf(".git/refs/heads/main"), "refs");
  assert.equal(gitChangeOf(".git/refs/tags/v1"), "refs");
  assert.equal(gitChangeOf(".git/refs/remotes/origin/main"), "refs");
  assert.equal(gitChangeOf(".git/packed-refs"), "refs");
  assert.equal(gitChangeOf(".git/FETCH_HEAD"), "refs");
  assert.equal(gitChangeOf(".git/refs/stash"), "stash");
  for (const marker of ["ORIG_HEAD", "MERGE_HEAD", "REBASE_HEAD", "CHERRY_PICK_HEAD", "REVERT_HEAD"]) assert.equal(gitChangeOf(`.git/${marker}`), "operation", marker);
  assert.equal(gitChangeOf(".git/rebase-merge/done"), "operation");
  assert.equal(gitChangeOf(".git/rebase-apply/0001"), "operation");
  assert.equal(gitChangeOf(".git/sequencer/todo"), "operation");
  assert.equal(gitChangeOf(".git/objects/ab/cdef"), null);
  assert.equal(gitChangeOf(".git/logs/HEAD"), null);
  assert.equal(gitChangeOf(".git/COMMIT_EDITMSG"), null);
  assert.equal(gitChangeOf(".git"), null);
  assert.equal(gitChangeOf(""), "worktree", "a rescan frame carries no path; its kind is read from the frame");
});

test("the watcher's own list and this rule agree: every .git name the engine reports has a kind", () => {
  const rust = readFileSync(new URL("../../../../crates/bisa-engine/src/ide/watch.rs", import.meta.url), "utf8");
  const start = rust.indexOf("fn worth_reporting");
  const body = rust.slice(start, rust.indexOf("\n}\n", start));
  const names = [...body.matchAll(/Some\("([^"]+)"\)/g)].map((m) => m[1]).filter((n) => n !== ".git");
  assert.ok(names.length >= 10, `the engine's list was read: ${names}`);
  for (const name of names) {
    const dir = ["refs", "rebase-merge", "rebase-apply", "sequencer"].includes(name);
    assert.notEqual(gitChangeOf(`.git/${name}${dir ? "/x" : ""}`), null, `.git/${name} has a kind`);
  }
});

test("a frame is one kind, a rescan every kind", () => {
  assert.deepEqual([...kindsOf({ path: "src/a.rs", kind: "modified" })], ["worktree"]);
  assert.deepEqual([...kindsOf({ path: ".git/objects/x", kind: "created" })], []);
  assert.deepEqual([...kindsOf({ path: "", kind: "rescan" })].sort(), [...GIT_CHANGES].sort());
});

test("each kind reaches the views that show it, and a burst is one window", () => {
  const none = readsFor([]);
  assert.deepEqual(none, { status: false, files: false, history: false, branches: false, stashes: false });
  assert.deepEqual(readsFor(["worktree"]), { status: true, files: true, history: false, branches: false, stashes: false });
  assert.deepEqual(readsFor(["index"]), { status: true, files: true, history: false, branches: false, stashes: false });
  assert.deepEqual(readsFor(["head"]), { status: true, files: true, history: true, branches: true, stashes: false });
  assert.deepEqual(readsFor(["refs"]), { status: true, files: false, history: true, branches: true, stashes: false }, "a fetch never re-lists the tree");
  assert.deepEqual(readsFor(["stash"]), { status: true, files: true, history: false, branches: false, stashes: true });
  assert.deepEqual(readsFor(["operation"]), { status: true, files: true, history: true, branches: true, stashes: false });
  assert.ok(GIT_COALESCE_MS >= 100 && GIT_COALESCE_MS <= 500, String(GIT_COALESCE_MS));
});
