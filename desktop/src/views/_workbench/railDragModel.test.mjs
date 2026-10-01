/**
 * The rail's drag contract. Run with `node --test desktop/src/views/_workbench/railDragModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { dropPlan } from "../../ui/tree/treeListModel.mjs";
import { railRows, treeRowsOf } from "./projectRailModel.mjs";
import { isHeadless, railCanNest, railCanReorder, railDragOf, railDropVerdict } from "./railDragModel.mjs";

const P = (id, name, extra = {}) => ({
  project: { id, slug: name.toLowerCase(), name, tags: [], root: { type: "managed" }, vcs: { type: "git", default_branch: "main" }, publish: "gated", revision: 1, created_at: 0, origin: { origin: "workspace" }, ...extra },
  path: `/ws/${id}`,
  exists: true,
  workstreams: 1,
  goals: [],
});
const W = (id, project, kind, created_at = 1, name = null) => ({
  project_name: null,
  path: `/ws/${id}`,
  exists: true,
  workstream: { id, project, name, note: null, pinned: false, kind, goal: null, work_item: null, agent: null, state: { state: "open" }, created_at },
});
const primary = (pid, at = 0) => W(pid, pid, { kind: "primary" }, at);
const worktree = (id, pid, branch, at) => W(id, pid, { kind: "worktree", branch, base: "main" }, at);
const T = (key, id) => ({ key, scope: "workstream", id, harness: null, label: null, resume: false, generation: 0, liveness: { status: "live", code: 0 }, restoring: false });

const open = () => false;
/** Two groups and the ungrouped rest, every project open — named so by-name order is the order given. */
const grouped = (order = {}) =>
  railRows({
    projects: [P("p1", "Alpha", { group: "Shop" }), P("p2", "Beta"), P("p3", "Gamma", { group: "Admin" }), P("p4", "Zeta", { group: "Admin" })],
    workstreams: [primary("p1"), primary("p2"), primary("p3"), primary("p4")],
    collapsed: open,
    order,
  });
/** Nobody made a group: every project at the top, no heading. */
const headless = (order = {}) =>
  railRows({
    projects: [P("p1", "Alpha"), P("p2", "Beta"), P("p3", "Gamma")],
    workstreams: [primary("p1"), primary("p2"), primary("p3"), worktree("w1", "p1", "feat/a", 2), worktree("w2", "p1", "feat/b", 3)],
    terminals: [T("t1", "p1"), T("t2", "p1")],
    collapsed: open,
    order,
  });
const find = (rows, kind, id) => rows.find((r) => r.kind === kind && (kind === "project" ? r.project.id === id : r.id === id));
const treeId = (rows, kind, id) => `${kind}:${find(rows, kind, id).id}`;
/** A drop the tree would plan, with the rail's own rules. */
const planOn = (rows, data, overKind, overId, ratio) => {
  const tree = treeRowsOf(rows, open);
  const active = `${data.kind}:${find(rows, data.kind, data.id).id}`;
  return dropPlan(tree, active, treeId(rows, overKind, overId), ratio, {
    canNest: (t) => railCanNest(data, t.row),
    canReorder: (p) => railCanReorder(data, p?.row ?? null, { headless: isHeadless(rows) }),
  });
};

test("a payload carries the record's id and its section; folds and the row being renamed carry nothing", () => {
  const rows = grouped();
  assert.deepEqual(railDragOf(find(rows, "project", "p3")), { type: "rail-row", kind: "project", id: "p3", ctx: "group:Admin", label: "Gamma" });
  assert.deepEqual(railDragOf(find(rows, "project", "p2")), { type: "rail-row", kind: "project", id: "p2", ctx: "ungrouped", label: "Beta" });
  assert.deepEqual(railDragOf(find(rows, "group", "Admin")), { type: "rail-row", kind: "group", id: "Admin", ctx: "", label: "Admin" });
  assert.equal(railDragOf(find(rows, "group", "")), null, "Other projects is a fold, not a thing");
  assert.equal(railDragOf(find(rows, "project", "p3"), { renaming: { kind: "project", id: "p3" } }), null);
  const h = headless();
  assert.deepEqual(railDragOf(find(h, "workstream", "w1")), { type: "rail-row", kind: "workstream", id: "w1", ctx: "p1", label: "feat/a" });
  assert.deepEqual(railDragOf(find(h, "terminal", "t1")).kind, "terminal");
  assert.equal(railDragOf(find(h, "workstream", "w1"), { renaming: { kind: "workstream", id: "w1" } }), null);
  const goals = railRows({ tab: "goals", projects: [P("g1", "Goalish", { origin: { origin: "goal", goal: "g" } })], workstreams: [primary("g1")], goals: [{ id: "g", label: "Ship" }], collapsed: open });
  assert.equal(railDragOf(find(goals, "goal", "g")), null, "a goal heading never drags");
  assert.equal(railDragOf(find(goals, "project", "g1")).ctx, "goal:g");
});

test("a workstream nests and reorders under its own project only; a shell under its own workstream", () => {
  const rows = headless();
  const w = railDragOf(find(rows, "workstream", "w1"));
  assert.ok(railCanNest(w, find(rows, "project", "p1")));
  assert.equal(railCanNest(w, find(rows, "project", "p2")), false);
  assert.ok(railCanReorder(w, find(rows, "project", "p1")));
  assert.equal(railCanReorder(w, find(rows, "project", "p2")), false);
  assert.equal(railCanReorder(w, null, { headless: true }), false);
  const t = railDragOf(find(rows, "terminal", "t1"));
  assert.ok(railCanNest(t, find(rows, "workstream", "p1")));
  assert.equal(railCanNest(t, find(rows, "workstream", "w1")), false);
  assert.ok(railCanReorder(t, find(rows, "workstream", "p1")));
  // The plan the tree draws: a bar, not a refusal.
  const plan = planOn(rows, w, "workstream", "w2", 0.9);
  assert.ok(plan && !plan.noop);
  const verdict = railDropVerdict({}, w, plan, rows, treeRowsOf(rows, open));
  assert.deepEqual(verdict, { kind: "order", order: { workstreams: { p1: ["p1", "w2", "w1"] } } });
});

test("a project reorders at the top of a headless rail and under its heading otherwise", () => {
  const h = headless();
  const p = railDragOf(find(h, "project", "p3"));
  assert.ok(isHeadless(h));
  assert.ok(railCanReorder(p, null, { headless: true }));
  const plan = planOn(h, p, "project", "p1", 0.1);
  assert.deepEqual(railDropVerdict({}, p, plan, h, treeRowsOf(h, open)), { kind: "order", order: { projects: ["p3", "p1", "p2"] } });
  const g = grouped();
  const q = railDragOf(find(g, "project", "p4"));
  assert.equal(isHeadless(g), false);
  assert.equal(railCanReorder(q, null, { headless: false }), false, "between the groups is nowhere for a project");
  assert.ok(railCanReorder(q, find(g, "group", "Admin")));
  assert.ok(railCanNest(q, find(g, "group", "Admin")));
  const above = planOn(g, q, "project", "p3", 0.1);
  assert.deepEqual(railDropVerdict({}, q, above, g, treeRowsOf(g, open)), { kind: "order", order: { projects: ["p4", "p3", "p1", "p2"] } }, "one global list, placed among the group's own");
});

test("a project dropped among another group's projects, or on its heading, moves house — and its own group is never a regroup", () => {
  const g = grouped();
  const tree = treeRowsOf(g, open);
  const q = railDragOf(find(g, "project", "p1"));
  assert.ok(railCanNest(q, find(g, "group", "Admin")), "another group takes it");
  assert.ok(railCanNest(q, find(g, "group", "")), "so does Other projects");
  const between = planOn(g, q, "project", "p3", 0.9);
  assert.deepEqual(railDropVerdict({}, q, between, g, tree), {
    kind: "regroup",
    project: "p1",
    group: "Admin",
    order: { projects: ["p3", "p1", "p4", "p2"] },
    words: "Moved to Admin.",
  });
  const onHeading = planOn(g, q, "group", "", 0.5);
  assert.deepEqual(railDropVerdict({}, q, onHeading, g, tree), {
    kind: "regroup",
    project: "p1",
    group: null,
    order: { projects: ["p3", "p4", "p2", "p1"] },
    words: "Removed from its group.",
  });
  // Inside its own group nothing moves house; already its last, nothing is said.
  const last = railDragOf(find(g, "project", "p4"));
  const own = planOn(g, last, "group", "Admin", 0.5);
  assert.equal(railDropVerdict({}, last, own, g, tree), null, "already the last of Admin");
  const first = railDragOf(find(g, "project", "p3"));
  assert.equal(railDropVerdict({}, first, planOn(g, first, "group", "Admin", 0.5), g, tree).kind, "order", "its own group is never a regroup");
});

test("a group reorders among the named groups; the order keeps what a collapsed fold hides", () => {
  const g = grouped();
  const tree = treeRowsOf(g, open);
  const d = railDragOf(find(g, "group", "Shop"));
  assert.ok(railCanReorder(d, null));
  assert.equal(railCanReorder(d, find(g, "group", "Admin")), false);
  const plan = planOn(g, d, "group", "Admin", 0.1);
  assert.deepEqual(railDropVerdict({}, d, plan, g, tree), { kind: "order", order: { groups: ["Shop", "Admin"] } });
  // A saved order names a project the rail has folded away: it keeps its place.
  const saved = { projects: ["p9", "p2"] };
  const q = railDragOf(find(g, "project", "p4"));
  const above = planOn(g, q, "project", "p3", 0.1);
  assert.deepEqual(railDropVerdict(saved, q, above, g, tree).order.projects, ["p9", "p2", "p4", "p3", "p1"]);
});

test("a shell's place counts shells only; a noop or a foreign payload is nothing", () => {
  const h = headless();
  const tree = treeRowsOf(h, open);
  const t = railDragOf(find(h, "terminal", "t2"));
  const plan = planOn(h, t, "terminal", "t1", 0.1);
  assert.deepEqual(railDropVerdict({}, t, plan, h, tree), { kind: "sessions", key: "t2", before: 0 });
  assert.equal(railDropVerdict({}, t, { ...plan, noop: true }, h, tree), null);
  assert.equal(railDropVerdict({}, { type: "path", path: "x" }, plan, h, tree), null);
  assert.equal(railCanNest({ type: "path" }, find(h, "project", "p1")), false);
});
