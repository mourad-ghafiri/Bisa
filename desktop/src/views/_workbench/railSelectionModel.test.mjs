/**
 * The rail's selection as the Board reads it. Run with
 * `node --test desktop/src/views/_workbench/railSelectionModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { boardScope, listedSelection, parseSelection, sameSelection, selectionOf } from "./railSelectionModel.mjs";

test("a kept selection reads back as it was, and what is no selection as nothing", () => {
  const project = { kind: "project", id: "p1", label: "Bisa" };
  const group = { kind: "group", label: "Shop", projects: ["p1", "p2"] };
  assert.deepEqual(parseSelection(JSON.parse(JSON.stringify(project))), project);
  assert.deepEqual(parseSelection(JSON.parse(JSON.stringify(group))), group);
  assert.deepEqual(parseSelection({ kind: "project", id: "p1" }), { kind: "project", id: "p1", label: "" }, "a row's project carries no name of its own");
  assert.deepEqual(parseSelection({ kind: "group", label: "Shop", projects: ["p1", 7, "p1", "", "p2"] }), group, "ids only, each once");
  for (const raw of [null, undefined, "p1", 3, [], {}, { kind: "workstream", id: "w1" }, { kind: "project" }, { kind: "project", id: "" }, { kind: "project", id: 7 }, { kind: "group", label: "Shop" }, { kind: "group", label: "Shop", projects: [] }, { kind: "group", label: "", projects: ["p1"] }, { kind: "group", projects: ["p1"] }]) {
    assert.equal(parseSelection(raw), null, JSON.stringify(raw) ?? String(raw));
  }
});

test("an id the workspace no longer lists selects nothing", () => {
  const project = { kind: "project", id: "p1", label: "Bisa" };
  assert.equal(listedSelection(project, ["p1", "p2"]), project, "a project still listed is the same selection");
  assert.equal(listedSelection(project, ["p2"]), null);
  assert.equal(listedSelection(project, []), null);
  assert.equal(listedSelection(null, ["p1"]), null);
  const group = { kind: "group", label: "Shop", projects: ["p1", "p2", "p3"] };
  assert.equal(listedSelection(group, new Set(["p1", "p2", "p3", "p4"])), group);
  assert.deepEqual(listedSelection(group, ["p1", "p3"]), { kind: "group", label: "Shop", projects: ["p1", "p3"] }, "a group keeps the projects that are still there");
  assert.equal(listedSelection(group, ["p9"]), null, "and selects nothing once none is");
});

test("a heading selects its projects, a project row itself, and a row standing in a project that project", () => {
  assert.deepEqual(selectionOf({ kind: "group", id: "Shop", label: "Shop", projects: ["p1", "p2", "p1"] }), { kind: "group", label: "Shop", projects: ["p1", "p2"] });
  assert.deepEqual(selectionOf({ kind: "goal", id: "g1", label: "Ship the cart", projects: ["p3"] }), { kind: "group", label: "Ship the cart", projects: ["p3"] });
  assert.deepEqual(selectionOf({ kind: "project", id: "ungrouped:p1", project: { id: "p1", name: "Bisa" } }), { kind: "project", id: "p1", label: "Bisa" });
  assert.deepEqual(selectionOf({ kind: "workstream", id: "w1", project: "p1" }), { kind: "project", id: "p1", label: "" });
  assert.deepEqual(selectionOf({ kind: "terminal", id: "t1", project: "p2" }), { kind: "project", id: "p2", label: "" });
  assert.deepEqual(selectionOf({ kind: "agent", id: "s1", project: "p2" }), { kind: "project", id: "p2", label: "" });
  assert.equal(selectionOf(null), null);
  assert.equal(selectionOf({ kind: "workstream", id: "w9" }), null, "a row standing nowhere selects nothing");
});

test("the Board's scope: all, one project by name, a group by its projects and count", () => {
  assert.deepEqual(boardScope(null), { projects: null, words: "All workstreams", all: true });
  const one = boardScope({ kind: "project", id: "p1", label: "Bisa" });
  assert.deepEqual([...one.projects], ["p1"]);
  assert.equal(one.words, "Bisa");
  assert.equal(one.all, false);
  assert.equal(boardScope({ kind: "project", id: "p1", label: "" }, (id) => (id === "p1" ? "Bisa" : null)).words, "Bisa", "a row's project named from the workspace");
  assert.equal(boardScope({ kind: "project", id: "abcdefgh", label: "" }).words, "project ·cdefgh", "an unknown project by its tail");
  const group = boardScope({ kind: "group", label: "Shop", projects: ["p1", "p2", "p3"] });
  assert.deepEqual([...group.projects], ["p1", "p2", "p3"]);
  assert.equal(group.words, "Shop · 3 projects");
  assert.equal(boardScope({ kind: "group", label: "Solo", projects: ["p1"] }).words, "Solo · 1 project");
});

test("two selections that mean the same cards are the same", () => {
  assert.ok(sameSelection(null, null));
  assert.ok(sameSelection({ kind: "project", id: "p1", label: "" }, { kind: "project", id: "p1", label: "Bisa" }), "a project is its id");
  assert.ok(!sameSelection({ kind: "project", id: "p1", label: "" }, null));
  assert.ok(sameSelection({ kind: "group", label: "Shop", projects: ["p1", "p2"] }, { kind: "group", label: "Shop", projects: ["p1", "p2"] }));
  assert.ok(!sameSelection({ kind: "group", label: "Shop", projects: ["p1"] }, { kind: "group", label: "Shop", projects: ["p1", "p2"] }));
  assert.ok(!sameSelection({ kind: "group", label: "Shop", projects: ["p1"] }, { kind: "project", id: "p1", label: "" }));
});
