import test from "node:test";
import assert from "node:assert/strict";
import { homeAfterLeaving, homeRoot, homeRoute, leavesRoot } from "./ideHomeModel.mjs";

const P = (id, name) => ({ project: { id, name, slug: name }, path: "", exists: true, workstreams: 1, goals: [] });
const W = (id, project, state = "open") => ({ project_name: null, path: null, exists: true, workstream: { id, project, kind: { kind: "primary" }, state: { state } } });

test("the last root wins while it exists; otherwise the first project's primary; otherwise nowhere", () => {
  const projects = [P("p2", "Zeta"), P("p1", "Alpha")];
  const workstreams = [W("p1", "p1"), W("p2", "p2"), W("w1", "p2")];
  assert.deepEqual(homeRoot("w1", projects, workstreams), { scope: "workstream", id: "w1" });
  assert.deepEqual(homeRoot("gone", projects, workstreams), { scope: "workstream", id: "p1" }, "Alpha by name");
  assert.deepEqual(homeRoot(null, projects, workstreams), { scope: "workstream", id: "p1" });
  assert.equal(homeRoot(null, [], []), null);
  const closed = [W("w1", "p2", "closed")];
  assert.deepEqual(homeRoot("w1", projects, closed), { scope: "workstream", id: "p1" }, "a closed workstream is not a home");
  assert.deepEqual(homeRoot(null, [P("p9", "Solo")], []), { scope: "workstream", id: "p9" }, "a project whose primary has not been listed yet still opens");
});

test("a home is an open workstream of a listed project: an archived project's checkouts are nobody's, remembered or not", () => {
  // `GET /workstreams` lists every project's checkouts, an archived one's too;
  // the live `projects` list is the one consulted.
  const live = [P("p1", "Alpha")];
  const workstreams = [W("p1", "p1"), W("p2", "p2"), W("w2", "p2")];
  assert.deepEqual(homeRoot("w2", live, workstreams), { scope: "workstream", id: "p1" }, "the remembered root was a checkout of a project since archived");
  assert.deepEqual(homeRoot("p2", live, workstreams), { scope: "workstream", id: "p1" }, "or its primary");
  assert.equal(homeRoot("w2", [], workstreams), null, "only archived projects left: the landing, never a hidden project's root");
  assert.equal(homeRoot(null, null, null), null, "lists not read yet");
});

test("the home after leaving a project is chosen with the project and its checkouts taken out", () => {
  const projects = [P("p2", "Zeta"), P("p1", "Alpha")];
  const workstreams = [W("p1", "p1"), W("p2", "p2"), W("w2", "p2")];
  // Standing on the only project: nowhere to go but the landing.
  assert.equal(homeAfterLeaving("p1", "p1", [P("p1", "Alpha")], [W("p1", "p1")]), null);
  // Standing on a checkout of Zeta and leaving Zeta: Alpha's primary, never Zeta's other checkout.
  assert.deepEqual(homeAfterLeaving("p2", "w2", projects, workstreams), { scope: "workstream", id: "p1" });
  assert.deepEqual(homeAfterLeaving("p2", "p2", projects, workstreams), { scope: "workstream", id: "p1" });
  // Leaving another project: the remembered root stands.
  assert.deepEqual(homeAfterLeaving("p1", "w2", projects, workstreams), { scope: "workstream", id: "w2" });
  // A remembered root that was already gone counts for nothing; the first remaining project's primary wins.
  assert.deepEqual(homeAfterLeaving("p1", "gone", projects, workstreams), { scope: "workstream", id: "p2" });
  // The facts in hand still list the leaving project (the lists are not read again first): it is taken out all the same.
  assert.equal(homeAfterLeaving("p1", null, [P("p1", "Alpha")], [W("p1", "p1"), W("w1", "p1")]), null);
});

test("a fact on the bus takes the root from under the person only when it puts away or removes the root's own project", () => {
  const workstreams = [W("p1", "p1"), W("w1", "p1"), W("p2", "p2")];
  const onCheckout = { scope: "workstream", id: "w1" };
  assert.equal(leavesRoot({ type: "project_deleted", project: "p1" }, onCheckout, workstreams), "p1", "the project deleted, standing on a checkout of it");
  assert.equal(leavesRoot({ type: "project_archived", project: "p1", archived: true }, { scope: "workstream", id: "p1" }, workstreams), "p1", "the project archived, standing on its primary");
  assert.equal(leavesRoot({ type: "project_archived", project: "p1", archived: false }, onCheckout, workstreams), null, "taken back out of the archive: nothing falls");
  assert.equal(leavesRoot({ type: "project_deleted", project: "p2" }, onCheckout, workstreams), null, "another project's fact");
  assert.equal(leavesRoot({ type: "project_deleted", project: "p1" }, { scope: "goal", id: "p1" }, workstreams), null, "a goal's root shares no id space");
  assert.equal(leavesRoot({ type: "project_deleted", project: "p1" }, null, workstreams), null, "no root");
  assert.equal(leavesRoot({ type: "settings_changed", scope: "project", project: "p1", keys: [] }, onCheckout, workstreams), null, "a fact that is neither");
  assert.equal(leavesRoot({ type: "project_deleted", project: "p9" }, { scope: "workstream", id: "w9" }, workstreams), null, "a root the list does not know is left standing: its own read answers not-found");
});

test("a home is a route to the workbench on it; none is the landing", () => {
  assert.deepEqual(homeRoute({ scope: "workstream", id: "p1" }), { name: "workbench", scope: "workstream", id: "p1" });
  assert.deepEqual(homeRoute(null), { name: "projects" });
});
