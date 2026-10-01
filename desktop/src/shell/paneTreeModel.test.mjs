import test from "node:test";
import assert from "node:assert/strict";
import { addTab, closeLeaf, dividers, leafOfTab, leaves, moveWithin, neighbor, normalize, parseTree, rects, removeTab, setActiveTab, setRatio, singleLeaf, splitLeaf } from "./paneTreeModel.mjs";

const two = () => {
  let t = singleLeaf("p1", ["t1", "t2"], "t2");
  t = splitLeaf(t, "p1", "row", "s1", "p2");
  t = addTab(t, "p2", "t3");
  return t;
};

test("a split keeps the tabs on the first side and the new leaf starts empty until a tab lands", () => {
  const t = splitLeaf(singleLeaf("p1", ["t1"]), "p1", "row", "s1", "p2");
  assert.equal(t.kind, "split");
  assert.deepEqual(leaves(t).map((l) => l.id), ["p1", "p2"]);
  assert.deepEqual(leaves(t)[1].tabs, []);
  // Normalizing an empty second leaf collapses the split back — the caller
  // adds a tab before that happens.
  assert.equal(normalize(t).kind, "leaf");
  const filled = addTab(t, "p2", "t9");
  assert.equal(filled.kind, "split");
  assert.equal(leafOfTab(filled, "t9").id, "p2");
  assert.equal(leaves(filled)[1].active, "t9");
});

test("moving the active tab into the new pane", () => {
  const t = splitLeaf(singleLeaf("p1", ["t1", "t2"], "t2"), "p1", "col", "s1", "p2", true);
  const [a, b] = leaves(t);
  assert.deepEqual(a.tabs, ["t1"]);
  assert.equal(a.active, "t1");
  assert.deepEqual(b.tabs, ["t2"]);
  // A single-tab leaf cannot give its only tab away.
  const same = singleLeaf("p1", ["t1"]);
  assert.equal(splitLeaf(same, "p1", "col", "s1", "p2", true), same);
});

test("every tab is in exactly one leaf, whatever you do", () => {
  let t = two();
  const every = (tree) => leaves(tree).flatMap((l) => l.tabs).sort();
  assert.deepEqual(every(t), ["t1", "t2", "t3"]);
  t = addTab(t, "p1", "t3");
  assert.deepEqual(every(t), ["t1", "t2", "t3"], "moving a tab does not duplicate it");
  assert.equal(leafOfTab(t, "t3").id, "p1");
  // p2 is now empty and collapsed away.
  assert.equal(t.kind, "leaf");
  // A hand-made tree with a duplicate is repaired, first occurrence wins.
  const dup = { kind: "split", id: "s", dir: "row", ratio: 0.5, a: singleLeaf("a", ["x", "y"]), b: singleLeaf("b", ["y", "z"]) };
  const fixed = normalize(dup);
  assert.deepEqual(leaves(fixed).map((l) => l.tabs), [["x", "y"], ["z"]]);
  assert.equal(normalize(fixed), fixed, "idempotent, identity-stable");
});

test("removing a tab activates the right neighbour then the left, and an emptied pane closes", () => {
  let t = singleLeaf("p1", ["t1", "t2", "t3"], "t2");
  t = removeTab(t, "t2");
  assert.equal(t.active, "t3");
  t = removeTab(t, "t3");
  assert.equal(t.active, "t1");
  const s = removeTab(two(), "t3");
  assert.equal(s.kind, "leaf", "the second pane had one tab; gone with it");
  assert.deepEqual(s.tabs, ["t1", "t2"]);
  const only = singleLeaf("p1", ["t1"]);
  const empty = removeTab(only, "t1");
  assert.deepEqual(empty.tabs, [], "the last leaf survives empty");
  assert.equal(removeTab(only, "nope"), only);
});

test("closing a pane moves its tabs to the neighbour rather than dropping them", () => {
  const t = two();
  const closed = closeLeaf(t, "p2");
  assert.equal(closed.kind, "leaf");
  assert.deepEqual(closed.tabs, ["t1", "t2", "t3"]);
  assert.equal(closed.active, "t2", "the surviving pane keeps its own active tab");
  const single = singleLeaf("p1", ["t1"]);
  assert.equal(closeLeaf(single, "p1"), single, "the only pane cannot close");
});

test("rects tile the unit square by the ratios; dividers sit at the cut", () => {
  let t = two();
  t = setRatio(t, "s1", 0.25);
  const r = rects(t);
  assert.deepEqual(r, [
    { leafId: "p1", x: 0, y: 0, w: 0.25, h: 1 },
    { leafId: "p2", x: 0.25, y: 0, w: 0.75, h: 1 },
  ]);
  const d = dividers(t);
  assert.equal(d.length, 1);
  assert.equal(d[0].dir, "row");
  assert.equal(d[0].at, 0.25);
  // Ratios are clamped so a pane never collapses to nothing.
  assert.equal(setRatio(t, "s1", 0.01).ratio, 0.15);
  assert.equal(setRatio(t, "s1", 0.99).ratio, 0.85);
  assert.equal(setRatio(t, "s1", 0.25), t, "unchanged ratio is the same object");
  // Nested: split p2 downwards.
  const nested = addTab(splitLeaf(t, "p2", "col", "s2", "p3"), "p3", "t4");
  const rr = rects(nested);
  assert.equal(rr.length, 3);
  const total = rr.reduce((acc, x) => acc + x.w * x.h, 0);
  assert.ok(Math.abs(total - 1) < 1e-9, "the rects tile the panel");
});

test("neighbours follow the geometry", () => {
  let t = two(); // p1 | p2
  t = addTab(splitLeaf(t, "p2", "col", "s2", "p3"), "p3", "t4"); // p2 above p3, both right of p1
  assert.equal(neighbor(t, "p1", "right"), "p2", "the larger overlap wins on a tie by order — p2 and p3 both touch; p2 is first");
  assert.equal(neighbor(t, "p2", "down"), "p3");
  assert.equal(neighbor(t, "p3", "up"), "p2");
  assert.equal(neighbor(t, "p3", "left"), "p1");
  assert.equal(neighbor(t, "p1", "left"), null);
  assert.equal(neighbor(t, "p2", "up"), null);
});

test("active tab bookkeeping", () => {
  const t = singleLeaf("p1", ["t1", "t2"], "t1");
  assert.equal(setActiveTab(t, "p1", "t2").active, "t2");
  assert.equal(setActiveTab(t, "p1", "t1"), t);
  assert.equal(setActiveTab(t, "p1", "nope"), t);
  assert.equal(setActiveTab(t, "px", "t2"), t);
});

test("a stored tree comes back with unknown tabs dropped and nonsense refused", () => {
  const t = two();
  const back = parseTree(JSON.parse(JSON.stringify(t)), ["t1", "t2", "t3"]);
  assert.deepEqual(back, t);
  const partial = parseTree(JSON.parse(JSON.stringify(t)), ["t1"]);
  assert.equal(partial.kind, "leaf");
  assert.deepEqual(partial.tabs, ["t1"]);
  assert.equal(parseTree(null, []), null);
  assert.equal(parseTree({ kind: "what" }, []), null);
  assert.equal(parseTree({ kind: "split", id: "s", dir: "diag", a: {}, b: {} }, []), null);
  const half = parseTree({ kind: "split", id: "s", dir: "row", ratio: 0.5, a: { kind: "leaf", id: "a", tabs: ["t1"] }, b: 7 }, ["t1"]);
  assert.equal(half.kind, "leaf", "a split with one unreadable side is its readable side");
});

test("a tab moves along its own leaf's strip and nowhere else; the active bit and the other leaf stay", () => {
  const t = addTab(two(), "p1", "t4");
  assert.deepEqual(leafOfTab(t, "t1").tabs, ["t1", "t2", "t4"]);
  const moved = moveWithin(t, "t4", 0);
  assert.deepEqual(leafOfTab(moved, "t4").tabs, ["t4", "t1", "t2"]);
  assert.equal(leafOfTab(moved, "t4").active, "t4", "the active tab is still the active tab");
  assert.deepEqual(leafOfTab(moved, "t3").tabs, ["t3"], "the other leaf did not notice");
  assert.equal(moveWithin(t, "t4", 2), t, "same place: same tree");
  assert.deepEqual(leafOfTab(moveWithin(t, "t1", 99), "t1").tabs, ["t2", "t4", "t1"], "past the end is the end");
  assert.equal(moveWithin(t, "nope", 0), t, "a tab the tree does not hold moves nothing");
});
