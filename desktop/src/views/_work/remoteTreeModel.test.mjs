/**
 * A remote's branches as a tree or a list, never with the remote's prefix.
 * Run with `node --test desktop/src/views/_work/remoteTreeModel.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";

import { branchTitle, countWords, foldableIds, folderId, remoteBranchRows, standingOf, wantsFilter } from "./remoteTreeModel.mjs";

const branch = (name, timestamp, over = {}) => ({ remote: "origin", name, head: `sha-${name}`, subject: `did ${name}`, timestamp, trackedBy: null, full: `origin/${name}`, ...over });
const group = (branches) => ({ remote: { name: "origin", url: "git@github.com:acme/shop.git" }, branches });
const shape = (rows) => rows.map((r) => [r.kind, r.depth, r.label, r.id]);
const NONE = new Set();

test("the tree is the explorer's shape: folders first then branches at each level, nested by their slashes, the basename alone on a row", () => {
  const rows = remoteBranchRows("w", group([branch("main", 10), branch("feature/x", 30), branch("feature/deep/y", 20), branch("release/1.0", 5)]), { layout: "tree", folded: NONE });
  assert.deepEqual(shape(rows), [
    ["dir", 0, "feature", "remote.w.origin.feature"],
    ["dir", 1, "deep", "remote.w.origin.feature/deep"],
    ["branch", 2, "y", "remote-branch:origin/feature/deep/y"],
    ["branch", 1, "x", "remote-branch:origin/feature/x"],
    ["dir", 0, "release", "remote.w.origin.release"],
    ["branch", 1, "1.0", "remote-branch:origin/release/1.0"],
    ["branch", 0, "main", "remote-branch:origin/main"],
  ]);
  assert.equal(rows[0].count, 2, "a folder counts every branch under it");
  assert.ok(rows.every((r) => !String(r.label).startsWith("origin")), "the remote's prefix is never on a row");
  assert.deepEqual(remoteBranchRows("w", group([]), { layout: "tree", folded: NONE }), []);
});

test("the list is every branch with its whole name, newest first", () => {
  const rows = remoteBranchRows("w", group([branch("main", 10), branch("feature/x", 30), branch("release/1.0", 5)]), { layout: "list", folded: NONE });
  assert.deepEqual(shape(rows), [
    ["branch", 0, "feature/x", "remote-branch:origin/feature/x"],
    ["branch", 0, "main", "remote-branch:origin/main"],
    ["branch", 0, "release/1.0", "remote-branch:origin/release/1.0"],
  ]);
});

test("a fold hides a folder's children and keeps the row; the fold id is the collapsed store's, per checkout and remote", () => {
  const g = group([branch("feature/x", 1), branch("feature/y", 2), branch("main", 3)]);
  const rows = remoteBranchRows("w", g, { layout: "tree", folded: new Set([folderId("w", "origin", "feature")]) });
  assert.deepEqual(shape(rows), [
    ["dir", 0, "feature", "remote.w.origin.feature"],
    ["branch", 0, "main", "remote-branch:origin/main"],
  ]);
  assert.equal(rows[0].expanded, false);
  assert.deepEqual(foldableIds("w", g), ["remote.w.origin.feature"]);
  assert.equal(folderId("w2", "fork", "a/b"), "remote.w2.fork.a/b");
});

test("a filter keeps the branches it matches and opens the folders holding one; nothing else is drawn", () => {
  const g = group([branch("feature/x", 1), branch("feature/y", 2), branch("main", 3), branch("hotfix/x-2", 4)]);
  const rows = remoteBranchRows("w", g, { layout: "tree", folded: new Set([folderId("w", "origin", "feature"), folderId("w", "origin", "hotfix")]), filter: "x" });
  assert.deepEqual(shape(rows), [
    ["dir", 0, "feature", "remote.w.origin.feature"],
    ["branch", 1, "x", "remote-branch:origin/feature/x"],
    ["dir", 0, "hotfix", "remote.w.origin.hotfix"],
    ["branch", 1, "x-2", "remote-branch:origin/hotfix/x-2"],
  ]);
  assert.ok(rows.filter((r) => r.kind === "dir").every((r) => r.expanded), "a hit is never behind a fold");
  assert.deepEqual(remoteBranchRows("w", g, { layout: "list", folded: NONE, filter: "zzz" }), []);
  assert.equal(wantsFilter([g]), false, "four branches need no filter");
  assert.equal(wantsFilter([g, g]), true);
});

test("a row's standing is the default branch and the local branch that tracks it; its tooltip is the whole name, the subject and the sha", () => {
  const rows = remoteBranchRows("w", group([branch("main", 1, { trackedBy: "main" }), branch("dev", 2)]), { layout: "list", folded: NONE, defaultBranch: "main" });
  assert.deepEqual(rows[1].standing, { isDefault: true, trackedBy: "main" });
  assert.deepEqual(rows[0].standing, { isDefault: false, trackedBy: null });
  assert.deepEqual(standingOf({ name: "x", trackedBy: "x" }), { isDefault: false, trackedBy: "x" });
  assert.equal(branchTitle({ remote: "origin", name: "main", subject: "Ship it", head: "0123456789abcdef" }), "origin/main · Ship it · 0123456789ab");
  assert.equal(branchTitle({ remote: "origin", name: "main" }), "origin/main");
  assert.equal(countWords(1), "1 branch");
  assert.equal(countWords(12), "12 branches");
});
