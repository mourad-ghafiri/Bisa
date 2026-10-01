/**
 * The rail's manual ordering as facts.
 * Run with `node --test desktop/src/views/_workbench/railOrderModel.test.mjs`.
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { groupOrder, orderBy, placeAmong, projectOrder, renameGroup, reorderAmong, workstreamOrder } from "./railOrderModel.mjs";

test("orderBy leads with the saved ids, then keeps the rest's incoming order", () => {
  const items = [{ id: "a" }, { id: "b" }, { id: "c" }, { id: "d" }];
  const out = orderBy(items, ["c", "a"], (x) => x.id).map((x) => x.id);
  assert.deepEqual(out, ["c", "a", "b", "d"], "saved first in its order; unlisted keep their order");
});

test("orderBy with no saved order is a no-op (input order preserved)", () => {
  const items = [{ id: "x" }, { id: "y" }];
  assert.deepEqual(orderBy(items, [], (x) => x.id).map((x) => x.id), ["x", "y"]);
  assert.deepEqual(orderBy(items, undefined, (x) => x.id).map((x) => x.id), ["x", "y"]);
});

test("renameGroup replaces the name in place, keeping its manual position", () => {
  const order = { groups: ["Shop", "Admin", "Dev"] };
  assert.deepEqual(groupOrder(renameGroup(order, "Admin", "Ops")), ["Shop", "Ops", "Dev"]);
  // No-ops when the group is not manually ordered or the names match.
  assert.equal(renameGroup(order, "Shop", "Shop"), order);
  assert.equal(renameGroup({ groups: ["Shop"] }, "Gone", "New").groups.includes("New"), false);
  assert.deepEqual(renameGroup({}, "a", "b"), {}, "no groups list: unchanged");
});

test("placeAmong puts a row where the drop plan points among one parent's children", () => {
  const all = ["a", "b", "c", "d"];
  assert.deepEqual(placeAmong(all, all, "d", 0), ["d", "a", "b", "c"], "before the first");
  assert.deepEqual(placeAmong(all, all, "a", 2), ["b", "c", "a", "d"], "index counts with the row itself removed");
  assert.deepEqual(placeAmong(all, all, "a", 3), ["b", "c", "d", "a"], "past the last is the end");
  assert.equal(placeAmong(all, all, "b", 1), all, "back where it was: the same list");
});

test("placeAmong across groups: a project dropped between two neighbours of another group takes the slot between them globally", () => {
  // Two groups drawn from one global list: Shop = [s1, s2], Dev = [d1, d2].
  const all = ["s1", "s2", "d1", "d2"];
  const dev = ["d1", "d2"];
  assert.deepEqual(placeAmong(all, dev, "s1", 1), ["s2", "d1", "s1", "d2"], "between d1 and d2");
  assert.deepEqual(placeAmong(all, dev, "s1", 0), ["s2", "s1", "d1", "d2"], "before d1");
  assert.deepEqual(placeAmong(all, dev, "s2", 2), ["s1", "d1", "d2", "s2"], "after the last of Dev");
  assert.deepEqual(placeAmong(all, [], "s1", 0), ["s2", "d1", "d2", "s1"], "into an empty group: the end of the list");
});

test("placeAmong tolerates siblings the saved order has never seen", () => {
  // A project created after the order was saved is drawn but not listed.
  assert.deepEqual(placeAmong(["a", "b"], ["a", "new", "b"], "b", 1), ["a", "b"], "before an unlisted sibling: after the listed one before it");
  assert.deepEqual(placeAmong(["a", "b"], ["a", "b"], "c", 1), ["a", "c", "b"], "an unlisted row itself is placed by its neighbours");
});

test("reorderAmong writes the dimension the plan names and is identity-stable on a no-op", () => {
  const order = { projects: ["p1", "p2"], groups: ["g1"] };
  const next = reorderAmong(order, { kind: "projects" }, ["p1", "p2", "p3"], ["p1", "p2", "p3"], "p3", 0);
  assert.deepEqual(projectOrder(next), ["p3", "p1", "p2"]);
  assert.deepEqual(next.groups, ["g1"]);
  assert.equal(reorderAmong(order, { kind: "projects" }, ["p1", "p2"], ["p1", "p2"], "p1", 0), order);
  const ws = reorderAmong({}, { kind: "workstreams", project: "p1" }, ["w1", "w2"], ["w1", "w2"], "w2", 0);
  assert.deepEqual(workstreamOrder(ws, "p1"), ["w2", "w1"]);
});
