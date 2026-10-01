/**
 * A flat list of paths as the tree the explorer draws: folders nested as on
 * disk, folders before files, the explorer's order, nothing compacted. Run
 * with `node --test desktop/src/ui/tree/pathTree.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";

import { filesUnder, orderedChildren, pathTree } from "./pathTree.mjs";

const pathOf = (p) => p;
const shape = (dir, depth = 0, out = []) => {
  for (const c of orderedChildren(dir, pathOf)) {
    if (c.kind === "dir") {
      out.push([depth, "dir", c.node.name, c.node.path]);
      shape(c.node, depth + 1, out);
    } else out.push([depth, "file", c.item]);
  }
  return out;
};

test("paths nest under their folders as on disk, folders before files, nothing compacted", () => {
  const tree = pathTree(["src/views/_work/b.mjs", "README.md", "src/ui/x.ts", "src/views/_work/a.mjs", "src/z.rs"], pathOf);
  assert.deepEqual(shape(tree), [
    [0, "dir", "src", "src"],
    [1, "dir", "ui", "src/ui"],
    [2, "file", "src/ui/x.ts"],
    [1, "dir", "views", "src/views"],
    [2, "dir", "_work", "src/views/_work"],
    [3, "file", "src/views/_work/a.mjs"],
    [3, "file", "src/views/_work/b.mjs"],
    [1, "file", "src/z.rs"],
    [0, "file", "README.md"],
  ]);
  assert.equal(tree.name, "");
  assert.equal(tree.path, "");
});

test("the order is the explorer's: numeric, case-blind, and stable on a tie", () => {
  const tree = pathTree(["b/10.log", "b/9.log", "b/README", "b/readme", "a.txt", "B.txt"], pathOf);
  assert.deepEqual(
    shape(tree).map((r) => r.at(-1)),
    ["b", "b/9.log", "b/10.log", "b/README", "b/readme", "a.txt", "B.txt"],
  );
});

test("a folder's files are every item under it, in the tree's order — never the folder itself", () => {
  const rows = [{ path: "src/b.rs" }, { path: "src/ui/a.ts" }, { path: "top.md" }];
  const tree = pathTree(rows, (r) => r.path);
  const src = tree.dirs.get("src");
  assert.deepEqual(filesUnder(src, (r) => r.path), [{ path: "src/ui/a.ts" }, { path: "src/b.rs" }]);
  assert.deepEqual(filesUnder(tree, (r) => r.path), [{ path: "src/ui/a.ts" }, { path: "src/b.rs" }, { path: "top.md" }]);
});

test("junk is skipped rather than placed", () => {
  const tree = pathTree([{ path: "" }, { path: null }, { path: "ok.txt" }], (r) => r.path);
  assert.deepEqual(filesUnder(tree, (r) => r.path), [{ path: "ok.txt" }]);
});
