import test from "node:test";
import assert from "node:assert/strict";
import { reconcileEdges, reconcileNodes, reuse, sameShallow } from "./flowMirrorModel.mjs";

const data = { label: "build" };
const node = (id, x = 0, extra = {}) => ({ id, position: { x, y: 0 }, type: "step", selected: false, draggable: true, connectable: true, data, ...extra });
const measured = (n) => ({ ...n, measured: { width: 220, height: 64 }, width: 220, height: 64 });

test("the same picture handed over again is the same array — nothing to commit", () => {
  const prev = [measured(node("a")), measured(node("b", 300))];
  const next = [node("a"), node("b", 300)];
  assert.equal(reconcileNodes(prev, next), prev, "a fresh array with equal content is not a change");
  const none = [];
  assert.equal(reconcileNodes(none, []), none, "an empty mirror handed an empty picture is the same empty array");
});

test("a moved, reselected or retyped node is a change, and every survivor keeps its measurement", () => {
  const prev = [measured(node("a")), measured(node("b", 300))];
  const moved = reconcileNodes(prev, [node("a", 40), node("b", 300)]);
  assert.notEqual(moved, prev);
  assert.equal(moved[0].position.x, 40);
  assert.deepEqual(moved[0].measured, { width: 220, height: 64 }, "the moved node is not re-measured");
  assert.deepEqual(moved[1].measured, { width: 220, height: 64 });
  assert.equal(moved[1].width, 220);
  assert.notEqual(reconcileNodes(prev, [node("a", 0, { selected: true }), node("b", 300)]), prev);
  assert.notEqual(reconcileNodes(prev, [node("a", 0, { type: "human" }), node("b", 300)]), prev);
  assert.notEqual(reconcileNodes(prev, [node("a", 0, { draggable: false }), node("b", 300)]), prev, "read-only flips draggable");
});

test("data is compared by reference: the caller memoises it, a new object is a change", () => {
  const prev = [measured(node("a"))];
  assert.equal(reconcileNodes(prev, [node("a")]), prev);
  assert.notEqual(reconcileNodes(prev, [node("a", 0, { data: { label: "build" } })]), prev);
});

test("an added, removed or reordered node is a change; a newcomer has no measurement yet", () => {
  const prev = [measured(node("a")), measured(node("b", 300))];
  const added = reconcileNodes(prev, [node("a"), node("b", 300), node("c", 600)]);
  assert.equal(added.length, 3);
  assert.equal(added[2].measured, undefined);
  assert.deepEqual(added[0].measured, { width: 220, height: 64 });
  assert.equal(reconcileNodes(prev, [node("a")]).length, 1);
  const swapped = reconcileNodes(prev, [node("b", 300), node("a")]);
  assert.deepEqual(swapped.map((n) => n.id), ["b", "a"], "next's order wins");
  assert.deepEqual(swapped[0].measured, { width: 220, height: 64 });
});

test("edges: the same content is the same array, anything else is next", () => {
  const edge = (id, extra = {}) => ({ id, source: "a", target: "b", sourceHandle: null, label: undefined, animated: false, className: "tone-default kind-then", type: "smoothstep", ...extra });
  const prev = [edge("e1"), edge("e2", { source: "b", target: "c" })];
  assert.equal(reconcileEdges(prev, [edge("e1"), edge("e2", { source: "b", target: "c" })]), prev);
  const next = [edge("e1", { animated: true }), edge("e2", { source: "b", target: "c" })];
  assert.equal(reconcileEdges(prev, next), next);
  assert.notEqual(reconcileEdges(prev, [edge("e1")]), prev);
  assert.notEqual(reconcileEdges(prev, [edge("e1", { selected: true }), prev[1]]), prev, "a selection xyflow reported and the caller echoed is a change");
});


test("reuse hands back the previous object when nothing about it changed", () => {
  const same = (a, b) => sameShallow(a, b, ["step", "problems"]);
  const step = { id: "a" };
  const prev = { step, problems: 0 };
  assert.equal(reuse(prev, { step, problems: 0 }, same), prev, "the same facts keep the old object");
  const next = { step, problems: 1 };
  assert.equal(reuse(prev, next, same), next, "a changed fact hands over the new one");
  assert.equal(reuse(undefined, next, same), next, "nothing before: the new one");
  // Shallow: values by identity, absence either way.
  assert.ok(sameShallow({ a: 1, b: null }, { a: 1, b: undefined }));
  assert.ok(!sameShallow({ a: 1 }, { a: 1, b: 2 }), "an extra key is a difference");
  assert.ok(!sameShallow({ a: { x: 1 } }, { a: { x: 1 } }), "a nested object is compared by identity");
  assert.ok(sameShallow(null, undefined));
  assert.ok(!sameShallow(null, {}));
  assert.ok(sameShallow({ state: "pending" }, { state: "pending" }), "a run state re-fetched with the same words is the same");
});
