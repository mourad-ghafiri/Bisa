/**
 * The tree's keyboard and drop rules. Run with `node --test desktop/src/ui/tree/treeListModel.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";

import { childrenOf, dragCue, dropPlan, dropSlots, indicatorStyle, keyAction, neighbourShift, parentsFromDepth, rowIndexOf, stepSlot, typeAhead } from "./treeListModel.mjs";

/**
 * src/            (open)
 *   main.rs
 *   lib/          (closed)
 *   listing…      (a message row)
 * docs/           (open)
 *   guide.md
 * README.md
 */
const ROWS = [
  { id: "src", depth: 0, label: "src", expandable: true, expanded: true },
  { id: "src/main.rs", depth: 1, label: "main.rs" },
  { id: "src/lib", depth: 1, label: "lib", expandable: true, expanded: false },
  { id: "loading:src", depth: 1, focusable: false },
  { id: "docs", depth: 0, label: "docs", expandable: true, expanded: true },
  { id: "docs/guide.md", depth: 1, label: "guide.md" },
  { id: "README.md", depth: 0, label: "README.md" },
];

test("a row's parent is the nearest row above it with a smaller depth", () => {
  const parents = parentsFromDepth(ROWS);
  assert.equal(parents.get("src"), null);
  assert.equal(parents.get("src/main.rs"), "src");
  assert.equal(parents.get("loading:src"), "src");
  assert.equal(parents.get("docs/guide.md"), "docs");
  assert.equal(parents.get("README.md"), null);
  assert.deepEqual(childrenOf(ROWS, parents, null), ["src", "docs", "README.md"]);
  assert.deepEqual(childrenOf(ROWS, parents, "src"), ["src/main.rs", "src/lib", "loading:src"]);
  assert.equal(rowIndexOf(ROWS, "docs"), 4);
  assert.equal(rowIndexOf(ROWS, "nope"), -1);
});

test("Down from nowhere lands on the first row and Up on the last; neither wraps", () => {
  assert.deepEqual(keyAction(ROWS, null, "ArrowDown"), { kind: "cursor", id: "src" });
  assert.deepEqual(keyAction(ROWS, null, "ArrowUp"), { kind: "cursor", id: "README.md" });
  assert.deepEqual(keyAction(ROWS, "README.md", "ArrowDown"), { kind: "cursor", id: "README.md" });
  assert.deepEqual(keyAction(ROWS, "src", "ArrowUp"), { kind: "cursor", id: "src" });
});

test("the cursor skips a message row", () => {
  assert.deepEqual(keyAction(ROWS, "src/lib", "ArrowDown"), { kind: "cursor", id: "docs" });
  assert.deepEqual(keyAction(ROWS, "docs", "ArrowUp"), { kind: "cursor", id: "src/lib" });
});

test("Home, End and the page keys", () => {
  assert.deepEqual(keyAction(ROWS, "docs", "Home"), { kind: "cursor", id: "src" });
  assert.deepEqual(keyAction(ROWS, "docs", "End"), { kind: "cursor", id: "README.md" });
  assert.deepEqual(keyAction(ROWS, "src", "PageDown", { page: 2 }), { kind: "cursor", id: "src/lib" });
  assert.deepEqual(keyAction(ROWS, "src", "PageDown", { page: 50 }), { kind: "cursor", id: "README.md" });
  assert.deepEqual(keyAction(ROWS, "README.md", "PageUp", { page: 2 }), { kind: "cursor", id: "docs" });
  assert.deepEqual(keyAction(ROWS, null, "PageUp", { page: 1 }), { kind: "cursor", id: "README.md" });
});

test("Right opens a closed folder and steps into an open one; on a file it is not the tree's", () => {
  assert.deepEqual(keyAction(ROWS, "src/lib", "ArrowRight"), { kind: "expand", id: "src/lib" });
  assert.deepEqual(keyAction(ROWS, "src", "ArrowRight"), { kind: "cursor", id: "src/main.rs" });
  assert.equal(keyAction(ROWS, "src/main.rs", "ArrowRight"), null);
  const emptyOpen = [{ id: "a", depth: 0, expandable: true, expanded: true }, { id: "b", depth: 0 }];
  assert.equal(keyAction(emptyOpen, "a", "ArrowRight"), null, "an open folder with nothing under it has nowhere to step");
});

test("Left closes an open folder, goes up from a child, and has nowhere to go at the top", () => {
  assert.deepEqual(keyAction(ROWS, "src", "ArrowLeft"), { kind: "collapse", id: "src" });
  assert.deepEqual(keyAction(ROWS, "src/main.rs", "ArrowLeft"), { kind: "cursor", id: "src" });
  assert.deepEqual(keyAction(ROWS, "src/lib", "ArrowLeft"), { kind: "cursor", id: "src" }, "a closed folder goes up, not shut again");
  assert.equal(keyAction(ROWS, "README.md", "ArrowLeft"), null);
});

test("* opens every closed sibling folder, and is nothing when none is closed", () => {
  assert.deepEqual(keyAction(ROWS, "src/main.rs", "*"), { kind: "expandAll", ids: ["src/lib"] });
  assert.equal(keyAction(ROWS, "src", "*"), null, "src and docs are already open");
});

test("Enter, Space, letters and a tree with no rows are left to the caller", () => {
  assert.equal(keyAction(ROWS, "src", "Enter"), null);
  assert.equal(keyAction(ROWS, "src", " "), null);
  assert.equal(keyAction(ROWS, "src", "a"), null);
  assert.equal(keyAction([], null, "ArrowDown"), null);
  assert.equal(keyAction([{ id: "m", depth: 0, focusable: false }], null, "ArrowDown"), null);
});

test("type-ahead finds the next label after the cursor, wraps, and skips message rows", () => {
  assert.equal(typeAhead(ROWS, null, "d"), "docs");
  assert.equal(typeAhead(ROWS, "src", "m"), "src/main.rs");
  assert.equal(typeAhead(ROWS, "docs", "s"), "src", "wraps round to the top");
  assert.equal(typeAhead(ROWS, "README.md", "re"), "README.md", "the cursor row itself when nothing else matches");
  assert.equal(typeAhead(ROWS, null, "zzz"), null);
  assert.equal(typeAhead(ROWS, null, ""), null);
});

test("a drop on a folder's middle goes inside it, at the end of its children", () => {
  assert.deepEqual(dropPlan(ROWS, "README.md", "src/lib", 0.5), { parent: "src/lib", index: 0, mode: "inside", over: "src/lib", depth: 2, noop: false });
  assert.deepEqual(dropPlan(ROWS, "README.md", "src", 0.4), { parent: "src", index: 3, mode: "inside", over: "src", depth: 1, noop: false });
});

test("a drop at a row's edges lands beside it, among the row's siblings", () => {
  assert.deepEqual(dropPlan(ROWS, "README.md", "docs", 0.1), { parent: null, index: 1, mode: "before", over: "docs", depth: 0, noop: false });
  assert.deepEqual(dropPlan(ROWS, "src/lib", "src/main.rs", 0.2), { parent: "src", index: 0, mode: "before", over: "src/main.rs", depth: 1, noop: false });
  assert.deepEqual(dropPlan(ROWS, "src/lib", "src/main.rs", 0.8), { parent: "src", index: 1, mode: "after", over: "src/main.rs", depth: 1, noop: true }, "just below main.rs is where lib already is");
});

test("just below an open folder with children means its first child", () => {
  assert.deepEqual(dropPlan(ROWS, "README.md", "docs", 0.9), { parent: "docs", index: 0, mode: "after", over: "docs", depth: 1, noop: false });
});

test("a row is never dropped on itself or into its own subtree", () => {
  assert.equal(dropPlan(ROWS, "src", "src", 0.5), null);
  assert.equal(dropPlan(ROWS, "src", "src/main.rs", 0.1), null);
  assert.equal(dropPlan(ROWS, "src", "src/lib", 0.5), null);
  assert.equal(dropPlan(ROWS, "src", "nope", 0.5), null, "an unknown row");
});

test("a foreign payload has no subtree to refuse and no place to be a no-op", () => {
  assert.deepEqual(dropPlan(ROWS, null, "src/lib", 0.5), { parent: "src/lib", index: 0, mode: "inside", over: "src/lib", depth: 2, noop: false });
  assert.deepEqual(dropPlan(ROWS, null, "src/main.rs", 0.9), { parent: "src", index: 1, mode: "after", over: "src/main.rs", depth: 1, noop: false });
});

test("canNest turns a folder into a plain row and canReorder refuses a parent", () => {
  const noNest = dropPlan(ROWS, "README.md", "src/lib", 0.5, { canNest: () => false });
  assert.deepEqual(noNest, { parent: "src", index: 2, mode: "after", over: "src/lib", depth: 1, noop: false }, "the middle half splits at 0.5 like any row");
  assert.equal(dropPlan(ROWS, "README.md", "src/main.rs", 0.1, { canReorder: (p) => p === null }), null, "nothing but the top level takes it");
  assert.deepEqual(dropPlan(ROWS, "docs", "src", 0.1, { canReorder: (p) => p === null }), { parent: null, index: 0, mode: "before", over: "src", depth: 0, noop: false });
});

test("dropping a row back beside itself is a plan that changes nothing", () => {
  assert.equal(dropPlan(ROWS, "docs", "src", 0.9, { canNest: () => false })?.noop, true, "below src is where docs already is");
  assert.equal(dropPlan(ROWS, "docs", "README.md", 0.1)?.noop, true, "above README is where docs already is");
  assert.equal(dropPlan(ROWS, "docs", "README.md", 0.9)?.noop, false);
});

test("several rows drag together: refused onto or into any of them, already there when every one sits under the target", () => {
  assert.equal(dropPlan(ROWS, ["src", "README.md"], "src/lib", 0.5), null, "into a dragged folder's subtree");
  assert.equal(dropPlan(ROWS, ["src", "README.md"], "README.md", 0.5), null, "onto a dragged row");
  assert.deepEqual(dropPlan(ROWS, ["src/main.rs", "README.md"], "docs", 0.5), { parent: "docs", index: 0, mode: "inside", over: "docs", depth: 1, noop: false });
  assert.equal(dropPlan(ROWS, ["src/main.rs", "src/lib"], "src", 0.5)?.noop, true, "both already under src");
  assert.equal(dropPlan(ROWS, ["src/main.rs", "README.md"], "src", 0.5)?.noop, false, "one of them is elsewhere");
  assert.equal(dropPlan(ROWS, ["docs", "README.md"], "src", 0.1)?.noop, true, "beside src at the top level is where both already are");
});

test("the one bar sits on the gap at the projected depth, and nowhere for a nest, a stay or the root's end", () => {
  const geom = { indent: 14, base: 6, rowTop: 100, rowHeight: 24 };
  const before = dropPlan(ROWS, "README.md", "src/main.rs", 0.1);
  assert.deepEqual(indicatorStyle(before, geom), { x: 20, y: 100 }, "before: the row's top, one level in");
  const after = dropPlan(ROWS, "README.md", "docs/guide.md", 0.9);
  assert.deepEqual(indicatorStyle(after, geom), { x: 20, y: 124 }, "after: the row's bottom");
  const firstChild = dropPlan(ROWS, "README.md", "src", 0.9);
  assert.equal(firstChild.depth, 1);
  assert.deepEqual(indicatorStyle(firstChild, geom), { x: 20, y: 124 }, "first child: below the open folder, one level in");
  assert.equal(indicatorStyle(dropPlan(ROWS, "README.md", "src/lib", 0.5), geom), null, "inside: the row lights, no bar");
  assert.equal(indicatorStyle(dropPlan(ROWS, "docs/guide.md", "docs/guide.md", 0.1) ?? { noop: true, mode: "before", over: "x", depth: 0 }, geom), null, "a stay draws nothing");
  assert.equal(indicatorStyle({ parent: null, index: 3, mode: "inside", over: "", depth: 0, noop: false }, geom), null);
  assert.equal(indicatorStyle(null, geom), null);
});

test("the cue says nothing elsewhere, refused with no plan, stay, nest or move", () => {
  assert.equal(dragCue(null, false), "none");
  assert.equal(dragCue(null, true), "refused");
  assert.equal(dragCue({ parent: null, index: 0, mode: "before", over: "src", depth: 0, noop: true }, true), "stay");
  assert.equal(dragCue({ parent: "src", index: 0, mode: "inside", over: "src", depth: 1, noop: false }, true), "nest");
  assert.equal(dragCue({ parent: null, index: 1, mode: "after", over: "src", depth: 0, noop: false }, true), "move");
});

test("the rows beside the gap part by two pixels each way; a nest or a stay parts nothing", () => {
  const before = { parent: "src", index: 0, mode: "before", over: "src/main.rs", depth: 1, noop: false };
  assert.equal(neighbourShift(before, "src/main.rs", "src", "src/lib"), 2, "the row under the bar drops");
  assert.equal(neighbourShift(before, "src", null, "src/main.rs"), -2, "the row above it rises");
  assert.equal(neighbourShift(before, "src/lib", "src/main.rs", "loading:src"), 0);
  const after = { parent: "src", index: 1, mode: "after", over: "src/main.rs", depth: 1, noop: false };
  assert.equal(neighbourShift(after, "src/main.rs", "src", "src/lib"), -2);
  assert.equal(neighbourShift(after, "src/lib", "src/main.rs", "loading:src"), 2);
  assert.equal(neighbourShift({ ...before, mode: "inside" }, "src/main.rs", "src", "src/lib"), 0);
  assert.equal(neighbourShift({ ...before, noop: true }, "src/main.rs", "src", "src/lib"), 0);
  assert.equal(neighbourShift(null, "src", null, null), 0);
});

test("a keyboard drag steps through every distinct slot top to bottom, and sideways changes the gap's depth", () => {
  const slots = dropSlots(ROWS, "src/main.rs");
  assert.ok(slots.length > 0);
  assert.deepEqual(slots.map((s) => [s.parent, s.index, s.mode]).slice(0, 4), [
    [null, 0, "before"],
    ["src", 2, "inside"],
    ["src", 0, "after"],
    ["src/lib", 0, "inside"],
  ]);
  const same = (a, b) => a.parent === b.parent && a.index === b.index;
  assert.equal(slots.filter((s) => same(s, { parent: "src", index: 1 }) && s.mode !== "inside").length, 1, "after main.rs and before lib are one slot");
  assert.ok(slots.some((s) => s.over === "" && s.parent === null), "the root's end is a slot");
  assert.ok(slots.some((s) => s.noop && s.parent === "src" && s.index === 0), "the slot the row already holds is among them");
  assert.deepEqual(stepSlot(ROWS, slots, null, "ArrowDown"), slots[0], "from nowhere, the first");
  assert.deepEqual(stepSlot(ROWS, slots, slots[0], "ArrowDown"), slots[1]);
  assert.deepEqual(stepSlot(ROWS, slots, slots[1], "ArrowUp"), slots[0]);
  assert.deepEqual(stepSlot(ROWS, slots, slots[0], "ArrowUp"), slots[0], "clamped, never wrapped");
  assert.deepEqual(stepSlot(ROWS, slots, slots.at(-1), "ArrowDown"), slots.at(-1));
  // Below the open `src`: as its first child (depth 1) or — one level out — nothing, since `src` is open with children.
  const belowSrc = slots.find((s) => s.over === "src" && s.mode === "after");
  assert.equal(belowSrc.depth, 1);
  assert.deepEqual(stepSlot(ROWS, slots, belowSrc, "ArrowLeft"), belowSrc, "no shallower slot on that gap: stays");
  // Below `docs/guide.md`: a sibling of it (depth 1) or, one level out, before `README.md` (depth 0) — the same line on screen.
  const afterGuide = slots.find((s) => s.over === "docs/guide.md" && s.mode === "after");
  const out = stepSlot(ROWS, slots, afterGuide, "ArrowLeft");
  assert.deepEqual([out.parent, out.index, out.depth], [null, 2, 0]);
  assert.deepEqual(stepSlot(ROWS, slots, out, "ArrowRight"), afterGuide, "and back in");
  assert.equal(stepSlot(ROWS, [], null, "ArrowDown"), null);
  assert.deepEqual(dropSlots(ROWS, "README.md", { canReorder: () => false }), [], "a tree that takes nothing has no slots");
  const end = dropSlots(ROWS, "README.md").find((s) => s.over === "");
  assert.equal(end.noop, true, "the last top-level row is already at the root's end");
});
