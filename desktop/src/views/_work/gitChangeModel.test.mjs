/**
 * What a frame means to git, and which view re-reads on it. Run with
 * `node --test desktop/src/views/_work/gitChangeModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { GIT_CHANGES, GIT_COALESCE_MS, MAX_IDLE_ROOTS, WRITES_TREE, gitChangeOf, idleRootsToDrop, kindsOf, readsFor, rereadsFor } from "./gitChangeModel.mjs";

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

test("the roots let go of are the ones left longest ago past the cap — never the ones read first — and a root shown, in flight or about to flush stays whatever its age", () => {
  const e = (subscribers, leftAt, over = {}) => ({ subscribers, inFlight: false, flush: null, leftAt, ...over });
  const entries = new Map([
    ["w:first", e(0, 5)],
    ["w:second", e(0, 1)],
    ["w:shown", e(1, 0)],
    ["w:reading", e(0, 2, { inFlight: true })],
    ["w:flushing", e(0, 3, { flush: 1 })],
    ["w:never", e(0, 0)],
    ["w:third", e(0, 4)],
  ]);
  assert.deepEqual(idleRootsToDrop(entries, 2), ["w:never", "w:second"], "never shown first, then the earliest leave; the two left last stay");
  assert.deepEqual(idleRootsToDrop(entries, 8), [], "under the cap, nothing goes");
  assert.deepEqual(idleRootsToDrop(entries, 0), ["w:never", "w:second", "w:third", "w:first"], "a cap of none lets every idle root go, oldest first");
  assert.deepEqual(idleRootsToDrop([], 8), []);
  assert.equal(MAX_IDLE_ROOTS, 8);
});

test("a frame re-reads every shown root after what writes into a tree with no frame of its own, one workstream's rows on a file_changed that touched its tree or index, and nothing on a ref move, a goal's folder, or a frame of no root", () => {
  assert.deepEqual([...WRITES_TREE], ["execution_ended", "result_accepted", "workstream_committed"]);
  for (const type of WRITES_TREE) assert.deepEqual(rereadsFor({ type }), { kind: "shown" }, type);
  assert.deepEqual(rereadsFor({ type: "file_changed", scope: "workstream", id: "w1", path: "src/a.rs", kind: "modified" }), { kind: "root", id: "w1" });
  assert.deepEqual(rereadsFor({ type: "file_changed", scope: "workstream", id: "w1", path: ".git/index", kind: "modified" }), { kind: "root", id: "w1" }, "the index is the rows' other half");
  assert.deepEqual(rereadsFor({ type: "file_changed", scope: "workstream", id: "w1", path: "", kind: "rescan" }), { kind: "root", id: "w1" }, "a rescan is every kind");
  assert.deepEqual(rereadsFor({ type: "file_changed", scope: "workstream", id: "w1", path: ".git/refs/remotes/origin/main", kind: "modified" }), { kind: "none" }, "a fetch that only moved refs re-lists nothing");
  assert.deepEqual(rereadsFor({ type: "file_changed", scope: "goal", id: "g1", path: "notes.md", kind: "modified" }), { kind: "none" }, "a goal's folder has no git rows");
  assert.deepEqual(rereadsFor({ type: "file_changed", scope: "workstream", id: "", path: "a", kind: "modified" }), { kind: "none" });
  assert.deepEqual(rereadsFor({ type: "workstream_changed" }), { kind: "none" }, "a record moving wrote nothing into the tree");
  assert.deepEqual(rereadsFor({}), { kind: "none" });
  assert.deepEqual(rereadsFor(null), { kind: "none" });
});
