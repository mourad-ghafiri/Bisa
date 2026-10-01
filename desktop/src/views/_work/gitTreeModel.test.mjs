/**
 * The Changes tree's rows: every changed file once, in the project's shape.
 * Run with `node --test desktop/src/views/_work/gitTreeModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { CONFLICTED_GROUP, actOn, changesTreeRows, deleteVerb, dirId, dirRowTitle, fileId, foldableIds, folderActions, gitFolderMenu, nextCursor, selectedIds, spaceVerb, toggledFold } from "./gitTreeModel.mjs";

const file = (path, over = {}) => ({ path, index: ".", worktree: "M", staged: false, unstaged: true, untracked: false, conflicted: false, ...over });
const untracked = (path) => file(path, { index: "?", worktree: "?", unstaged: true, untracked: true });
const staged = (path) => file(path, { index: "A", worktree: ".", staged: true, unstaged: false });
const both = (path) => file(path, { index: "M", worktree: "M", staged: true, unstaged: true });
const conflicted = (path) => file(path, { index: "U", worktree: "U", staged: false, unstaged: false, conflicted: true });
const shape = (rows) => rows.map((r) => [r.kind, r.depth, r.label, r.id]);
const NONE = new Set();

test("the tree layout is the project's own folders — folders first then files at each level, nested as on disk, nothing compacted, no section", () => {
  const rows = changesTreeRows([file("src/views/_work/b.mjs"), staged("src/views/_work/a.mjs"), untracked("src/ui/x.ts"), file("README.md")], "tree", NONE);
  assert.deepEqual(shape(rows), [
    ["dir", 0, "src", "dir:src"],
    ["dir", 1, "ui", "dir:src/ui"],
    ["file", 2, "x.ts", "file:src/ui/x.ts"],
    ["dir", 1, "views", "dir:src/views"],
    ["dir", 2, "_work", "dir:src/views/_work"],
    ["file", 3, "a.mjs", "file:src/views/_work/a.mjs"],
    ["file", 3, "b.mjs", "file:src/views/_work/b.mjs"],
    ["file", 0, "README.md", "file:README.md"],
  ]);
  const src = rows.find((r) => r.id === dirId("src"));
  assert.deepEqual(src.paths, ["src/ui/x.ts", "src/views/_work/a.mjs", "src/views/_work/b.mjs"], "a folder carries its files, in the tree's order");
  assert.equal(src.count, 3);
  assert.deepEqual(src.stageable, ["src/ui/x.ts", "src/views/_work/b.mjs"], "what Stage takes: the untracked and the unstaged");
  assert.deepEqual(src.unstageable, ["src/views/_work/a.mjs"]);
  assert.deepEqual(src.discardable, ["src/views/_work/b.mjs"]);
  assert.deepEqual(src.deletable, ["src/ui/x.ts"]);
  assert.equal(rows.find((r) => r.id === "file:README.md").row.path, "README.md", "a file row carries git's row");
  assert.ok(rows.every((r) => r.kind !== "section"), "no section: the tree is the project's");
  assert.deepEqual(changesTreeRows([], "tree", NONE), []);
  assert.deepEqual(changesTreeRows(null, "tree", NONE), []);
});

test("the conflicted paths come first — under one Conflicted group in the tree, ahead of the rest in the list — so what is left to settle is never hunted for", () => {
  const files = [file("src/b.rs"), conflicted("src/z.rs"), conflicted("a.rs"), staged("README.md")];
  const rows = changesTreeRows(files, "tree", NONE);
  assert.deepEqual(shape(rows).slice(0, 3), [
    ["dir", 0, "Conflicted", CONFLICTED_GROUP],
    ["file", 1, "a.rs", "file:a.rs"],
    ["file", 1, "src/z.rs", "file:src/z.rs"],
  ]);
  const group = rows[0];
  assert.equal(group.group, "conflicted");
  assert.deepEqual(group.paths, ["a.rs", "src/z.rs"]);
  assert.deepEqual(group.discardable, ["a.rs", "src/z.rs"], "Discard puts an unmerged path back to HEAD");
  assert.ok(rows.slice(3).every((r) => r.kind !== "file" || !r.row.conflicted), "the rest of the tree holds no conflicted path");
  assert.ok(shape(rows).some((r) => r[3] === "dir:src"), "the project's folders follow");
  assert.equal(changesTreeRows(files, "tree", new Set([CONFLICTED_GROUP]))[1].kind, "dir", "folded: the group alone, then the folders");
  assert.deepEqual(
    changesTreeRows(files, "list", NONE).map((r) => r.label),
    ["a.rs", "src/z.rs", "README.md", "src/b.rs"],
  );
  assert.ok(!changesTreeRows([file("x")], "tree", NONE).some((r) => r.id === CONFLICTED_GROUP), "no conflict: no group");
});

test("a file staged and edited since is one row with one standing, whatever git said twice", () => {
  const rows = changesTreeRows([both("a.rs"), both("a.rs")], "list", NONE);
  assert.deepEqual(shape(rows), [["file", 0, "a.rs", "file:a.rs"]]);
  assert.deepEqual(rows[0].standing, { staged: true, unstaged: true, untracked: false, conflicted: false, primary: "unstaged" });
  assert.deepEqual([...selectedIds({ path: "a.rs", staged: true })], ["file:a.rs"]);
  assert.deepEqual([...selectedIds({ path: "a.rs", staged: false })], ["file:a.rs"], "one row whichever side is open — the chip shows the side");
  assert.equal(selectedIds(null).size, 0);
});

test("the list layout is every file with its full path at depth 0, by path", () => {
  assert.deepEqual(shape(changesTreeRows([file("src/b.rs"), staged("src/a.rs"), untracked("new.txt")], "list", NONE)), [
    ["file", 0, "new.txt", "file:new.txt"],
    ["file", 0, "src/a.rs", "file:src/a.rs"],
    ["file", 0, "src/b.rs", "file:src/b.rs"],
  ]);
});

test("a fold hides a folder's children and keeps the row; its id is its full path, so it holds when a sibling appears", () => {
  const folded = new Set([dirId("src/views")]);
  const rows = changesTreeRows([file("src/views/a.mjs")], "tree", folded);
  assert.deepEqual(shape(rows), [
    ["dir", 0, "src", "dir:src"],
    ["dir", 1, "views", "dir:src/views"],
  ]);
  assert.equal(rows[1].expanded, false);
  const after = [file("src/views/a.mjs"), file("src/ui/b.ts")];
  assert.deepEqual(shape(changesTreeRows(after, "tree", folded)), [
    ["dir", 0, "src", "dir:src"],
    ["dir", 1, "ui", "dir:src/ui"],
    ["file", 2, "b.ts", "file:src/ui/b.ts"],
    ["dir", 1, "views", "dir:src/views"],
  ]);
  assert.deepEqual(toggledFold([], "x", false), ["x"]);
  assert.deepEqual(toggledFold(["x", "y"], "x", true), ["y"]);
  assert.deepEqual(toggledFold(["x"], "x", false), ["x"], "folding twice is once");
  assert.deepEqual(foldableIds(after), ["dir:src", "dir:src/ui", "dir:src/views"], "every folder, folded or not");
});

test("the cursor follows a row that is still there, else lands on its old place, else nowhere", () => {
  const before = [{ id: "a" }, { id: "b" }, { id: "c" }];
  assert.equal(nextCursor(before, "b", [{ id: "a" }, { id: "b" }]), "b");
  assert.equal(nextCursor(before, "b", [{ id: "a" }, { id: "c" }]), "c", "the row at the old index");
  assert.equal(nextCursor(before, "c", [{ id: "a" }]), "a", "clamped to the last row");
  assert.equal(nextCursor(before, "c", []), null);
  assert.equal(nextCursor(before, null, before), null);
  assert.equal(nextCursor(before, "zzz", before), null, "a cursor that was never a row");
});

test("a verb acts on a file's path when it applies, and on the files under a folder it applies to, naming the folder", () => {
  const rows = changesTreeRows([file("src/a.rs"), staged("src/b.rs"), untracked("src/n.txt"), both("src/c.rs")], "tree", NONE);
  assert.deepEqual(actOn(rows, fileId("src/a.rs"), "stage"), { kind: "file", paths: ["src/a.rs"], under: null });
  assert.equal(actOn(rows, fileId("src/a.rs"), "unstage"), null, "nothing staged to take back");
  assert.deepEqual(actOn(rows, fileId("src/c.rs"), "unstage"), { kind: "file", paths: ["src/c.rs"], under: null });
  assert.deepEqual(actOn(rows, fileId("src/n.txt"), "delete"), { kind: "file", paths: ["src/n.txt"], under: null });
  assert.equal(actOn(rows, fileId("src/n.txt"), "discard"), null);
  assert.deepEqual(actOn(rows, dirId("src"), "stage"), { kind: "dir", paths: ["src/a.rs", "src/c.rs", "src/n.txt"], under: "src" });
  assert.deepEqual(actOn(rows, dirId("src"), "unstage"), { kind: "dir", paths: ["src/b.rs", "src/c.rs"], under: "src" });
  assert.deepEqual(actOn(rows, dirId("src"), "discard"), { kind: "dir", paths: ["src/a.rs", "src/c.rs"], under: "src" });
  assert.deepEqual(actOn(rows, dirId("src"), "delete"), { kind: "dir", paths: ["src/n.txt"], under: "src" });
  assert.equal(actOn(rows, "nope", "stage"), null);
  assert.equal(actOn(rows, null, "stage"), null);
});

test("Space stages the primary side and unstages a file that is staged alone; Delete deletes an untracked file, discards a working-tree change and nothing on a staged-only one", () => {
  const rows = changesTreeRows([file("a.rs"), staged("b.rs"), both("c.rs"), untracked("n.txt"), conflicted("x.rs")], "list", NONE);
  const byId = (id) => rows.find((r) => r.id === id);
  assert.equal(spaceVerb(byId("file:a.rs")), "stage");
  assert.equal(spaceVerb(byId("file:b.rs")), "unstage");
  assert.equal(spaceVerb(byId("file:c.rs")), "stage", "a file in both stages its working-tree side");
  assert.equal(spaceVerb(byId("file:n.txt")), "stage");
  assert.equal(spaceVerb(byId("file:x.rs")), "stage", "staging an unmerged path marks it resolved — on purpose, by hand");
  assert.equal(deleteVerb(byId("file:a.rs")), "discard");
  assert.equal(deleteVerb(byId("file:b.rs")), null);
  assert.equal(deleteVerb(byId("file:c.rs")), "discard");
  assert.equal(deleteVerb(byId("file:n.txt")), "delete");
  assert.equal(deleteVerb(byId("file:x.rs")), "discard");
  const tree = changesTreeRows([staged("d/b.rs")], "tree", NONE);
  assert.equal(spaceVerb(tree[0]), "unstage", "a folder of staged files unstages");
  assert.equal(deleteVerb(tree[0]), null);
});

test("a folder reveals only the verbs it has files for, each naming the count, a throw-away red; its menu the same", () => {
  const dir = { path: "src", stageable: ["a", "b"], unstageable: ["c"], discardable: ["a"], deletable: ["b"] };
  const actions = folderActions(dir);
  assert.deepEqual(actions.map((a) => a.id), ["stage", "unstage", "discard", "delete"]);
  assert.equal(actions[0].label("src"), "Stage 2 under src/");
  assert.equal(actions[1].label("src"), "Unstage 1 under src/");
  for (const a of actions) {
    assert.equal(a.icon, a.id);
    assert.match(a.label("src/views"), /src\/views\//, `${a.id} names the folder`);
    assert.equal(a.tone, a.id === "discard" || a.id === "delete" ? "danger" : "quiet");
  }
  assert.deepEqual(folderActions({ path: "s", stageable: [], unstageable: ["c"], discardable: [], deletable: [] }).map((a) => a.id), ["unstage"], "a staged folder carries the toggle alone");
  assert.match(actions[3].hint, /folder and anything tracked in it stay/);
  const menu = (d, desktop = true) => gitFolderMenu(d, { desktop }).map((i) => i.id);
  assert.deepEqual(menu(dir), ["stage", "unstage", "discard", "delete", "copy-path", "copy-absolute", "reveal-files"]);
  assert.deepEqual(menu({ stageable: [], unstageable: ["c"], discardable: [], deletable: [] }), ["unstage", "copy-path", "copy-absolute", "reveal-files"]);
  assert.deepEqual(menu({ stageable: ["n"], unstageable: [], discardable: [], deletable: ["n"] }, false), ["stage", "delete", "copy-path", "reveal-files"]);
  const items = gitFolderMenu(dir, { desktop: true });
  assert.equal(items.find((i) => i.id === "stage").label, "Stage 2");
  assert.equal(items.find((i) => i.id === "delete").label, "Delete files…");
  assert.ok(items.find((i) => i.id === "discard").separatorBefore && !items.find((i) => i.id === "delete").separatorBefore, "the throw-aways are one group");
});

test("a folder row's tooltip counts what it holds, and the conflicted group says what follows", () => {
  assert.equal(dirRowTitle({ group: "conflicted", path: "", count: 1 }), "1 conflicted file — settle each, then the Resolve card continues");
  assert.equal(dirRowTitle({ group: "conflicted", path: "", count: 3 }), "3 conflicted files — settle each, then the Resolve card continues");
  assert.equal(dirRowTitle({ group: null, path: "src/lib", count: 1 }), "src/lib/ — 1 changed file");
  assert.equal(dirRowTitle({ path: "src", count: 12 }), "src/ — 12 changed files");
});
