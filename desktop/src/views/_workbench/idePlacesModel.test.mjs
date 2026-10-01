/**
 * The places the Project IDE keeps its view under (`idePlacesModel.mjs`).
 * Run with `node --test desktop/src/views/_workbench/idePlacesModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { gonePaths } from "../../shell/gonePlacesModel.mjs";
import { href } from "../../routeModel.mjs";
import { BOARD_PLACE, RAIL_PLACE, docsPrefix, gitPlace, idePlace, placesOfRoot, rootOfPath } from "./idePlacesModel.mjs";
import { rootKey, tabId } from "./workbenchModel.mjs";

/** A document's key as `editorRegistry.editorKey` makes it: the root, a bar, the tab's id. */
const editorKey = (root, tab) => `${root}|${tabId(tab)}`;

test("a root keeps its view under its scope and id, and its Git panel beside it", () => {
  assert.equal(idePlace(rootKey("workstream", "01W")), "ide:workstream:01W");
  assert.equal(idePlace(rootKey("goal", "01G")), "ide:goal:01G");
  assert.equal(gitPlace(rootKey("workstream", "01W")), "git:workstream:01W");
  assert.equal(RAIL_PLACE, "rail");
  assert.equal(BOARD_PLACE, "board", "one board, whatever root it is opened from");
});

test("a path of the IDE names its root, and no other path names one", () => {
  assert.equal(rootOfPath("/projects/workstream/01W"), rootKey("workstream", "01W"));
  assert.equal(rootOfPath("/projects/goal/01G"), rootKey("goal", "01G"));
  // The router's own spelling of a root is read back.
  assert.equal(rootOfPath(href({ name: "workbench", scope: "workstream", id: "01W" }).slice(1)), "workstream:01W");
  for (const other of ["/projects", "/projects/workstream", "/projects/workstream/", "/projects/workstream/01W/more", "/goals/01G", "/project/workstream/01W", "projects/workstream/01W", "/projects/Work/01W", "/projects/workstream/01W?doc=a", "", null, undefined, 3]) {
    assert.equal(rootOfPath(other), null, String(other));
  }
});

test("what a fact says is gone takes the root that stood on it", () => {
  const roots = (payload) => gonePaths(payload).map(rootOfPath).filter(Boolean);
  assert.deepEqual(roots({ type: "goal_deleted", goal: "01G" }), ["goal:01G"], "the goal's page is no root; its tree in the IDE is");
  assert.deepEqual(roots({ type: "project_deleted", project: "01P" }), ["workstream:01P"]);
  assert.deepEqual(roots({ type: "workstream_changed", workstream: "01S", state: { state: "closed" } }), ["workstream:01S"]);
  assert.deepEqual(roots({ type: "workflow_deleted", workflow: "01W" }), []);
});

test("a root that is forgotten takes its own place and its Git panel's, and no other root's", () => {
  assert.deepEqual(placesOfRoot("workstream:01W"), ["ide:workstream:01W", "git:workstream:01W"]);
  assert.ok(!placesOfRoot("workstream:01W").includes(idePlace("workstream:01W2")));
});

test("every document of a root is kept under the root's prefix, and another root's is not", () => {
  const prefix = docsPrefix("workstream:01W");
  assert.ok(editorKey("workstream:01W", { kind: "file", path: "src/a.ts" }).startsWith(prefix));
  assert.ok(!editorKey("workstream:01W2", { kind: "file", path: "src/a.ts" }).startsWith(prefix), "a root whose id begins the same is another root");
  const source = readFileSync(new URL("./editorRegistry.ts", import.meta.url), "utf8");
  assert.ok(source.includes("return `${rootKey}|${tabId}`;"), "the key's shape is the registry's");
});
