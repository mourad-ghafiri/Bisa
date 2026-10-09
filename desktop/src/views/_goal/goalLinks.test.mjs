/**
 * Where a path said about a goal is looked up: the primary checkouts of the
 * projects attached to it, and only those that exist. Run with
 * `node --test desktop/src/views/_goal/goalLinks.test.mjs`.
 */

import { strict as assert } from "node:assert";
import { test } from "node:test";

import { goalLinkRoots } from "./goalLinks.mjs";

test("a goal's link roots are the existing projects attached to it, as workstreams, in order", () => {
  const projects = [
    { exists: true, goals: ["g1", "g2"], project: { id: "p-a" } },
    { exists: false, goals: ["g1"], project: { id: "p-gone" } },
    { exists: true, goals: ["g2"], project: { id: "p-b" } },
    { exists: true, goals: ["g1"], project: { id: "p-c" } },
  ];
  assert.deepEqual(goalLinkRoots(projects, "g1"), [
    { scope: "workstream", id: "p-a" },
    { scope: "workstream", id: "p-c" },
  ]);
  assert.deepEqual(goalLinkRoots(projects, "nobody"), []);
  assert.deepEqual(goalLinkRoots(undefined, "g1"), [], "no projects is no roots");
});
