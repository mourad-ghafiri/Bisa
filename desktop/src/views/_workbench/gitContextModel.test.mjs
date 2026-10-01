/**
 * The checkout's changes as chips, tested where the rule lives: one chip per
 * hunk with the drag's id, a file chip for an untracked file, the budget
 * respected in order, and the words. Run with `npm test` from `desktop/`.
 */

import { strict as assert } from "node:assert";
import { test } from "node:test";

import { MAX_CONTEXT_BYTES, hunkChip, sameChip } from "./contextChips.mjs";
import { attachWithin, attachedWords, changeChips, fileChips, hunkId } from "./gitContextModel.mjs";

const patch = (path, ...starts) =>
  [`diff --git a/${path} b/${path}`, `--- a/${path}`, `+++ b/${path}`, ...starts.map((s) => `@@ -${s},1 +${s},2 @@\n line\n+added ${s}`)].join("\n") + "\n";

test("a changed file is one chip per hunk, staged before unstaged, each with the id a dragged hunk carries", () => {
  const chips = fileChips({ path: "src/a.rs", untracked: false }, { staged: patch("src/a.rs", 10), unstaged: patch("src/a.rs", 40, 90) });
  assert.deepEqual(
    chips.map((c) => [c.kind, c.scope.scope, c.hunk]),
    [
      ["diff_hunk", "staged", "src/a.rs@10"],
      ["diff_hunk", "unstaged", "src/a.rs@40"],
      ["diff_hunk", "unstaged", "src/a.rs@90"],
    ],
  );
  assert.match(chips[0].patch, /^@@ -10,1 \+10,2 @@/);
  assert.equal(hunkId("src/a.rs", { newStart: 40 }), "src/a.rs@40");
  // The same thing as the drag's chip: attaching after dragging replaces, never stacks.
  assert.ok(sameChip(chips[1], hunkChip("src/a.rs", false, "…", "src/a.rs@40")));
  assert.deepEqual(fileChips({ path: "x", untracked: false }, {}), [], "no patch, no chip");
  assert.deepEqual(fileChips({ path: "x", untracked: false }, { unstaged: "" }), []);
});

test("an untracked file has no patch and is a file chip; the listing's order is kept", () => {
  const rows = [
    { path: "b.md", untracked: false },
    { path: "new.txt", untracked: true },
    { path: "a.rs", untracked: false },
  ];
  const patches = new Map([
    ["a.rs", { unstaged: patch("a.rs", 1) }],
    ["b.md", { staged: patch("b.md", 5) }],
    ["new.txt", { unstaged: patch("new.txt", 1) }],
  ]);
  const chips = changeChips(rows, patches);
  assert.deepEqual(
    chips.map((c) => (c.kind === "file" ? `file ${c.path}` : c.hunk)),
    ["b.md@5", "file new.txt", "a.rs@1"],
    "an untracked file ignores any patch handed to it",
  );
});

test("attaching keeps the prefix that fits the budget, replaces a chip already in the tray, and counts the rest", () => {
  const small = (n) => hunkChip("f.rs", false, `@@ -${n},1 +${n},1 @@\n x`, `f.rs@${n}`);
  const tray = [small(1)];
  const { tray: next, attached, leftOut } = attachWithin(tray, [small(1), small(2)]);
  assert.equal(leftOut, 0);
  assert.equal(next.length, 2, "the chip already there is replaced, not doubled");
  assert.equal(attached.length, 2);
  // A chip that alone busts the budget stops the run there; everything after it is counted, not tried.
  const huge = hunkChip("big.rs", false, "x".repeat(MAX_CONTEXT_BYTES), "big.rs@1");
  const r = attachWithin([], [small(3), huge, small(4)]);
  assert.deepEqual(r.attached.map((c) => c.hunk), ["f.rs@3"]);
  assert.equal(r.leftOut, 2, "in order: nothing skips ahead of a chip that did not fit");
  assert.equal(r.tray.length, 1);
});

test("the words say what went in, from how many files, and what was left out", () => {
  const h = (p, n) => hunkChip(p, false, "…", `${p}@${n}`);
  assert.equal(attachedWords([h("a", 1), h("a", 2), h("b", 1)], 0), "Attached 3 hunks from 2 files.");
  assert.equal(attachedWords([h("a", 1)], 2), "Attached 1 hunk from 1 file — 2 left out: over the 64 KiB context budget.");
  assert.equal(attachedWords([{ kind: "file", path: "new.txt" }], 0), "Attached 1 untracked file.");
  assert.equal(attachedWords([h("a", 1), { kind: "file", path: "n" }, { kind: "file", path: "m" }], 0), "Attached 1 hunk and 2 untracked files from 1 file.");
  assert.equal(attachedWords([], 0), "Nothing to attach — the checkout has no changes.");
  assert.equal(attachedWords([], 3), "Attached nothing — 3 left out: over the 64 KiB context budget.");
});
