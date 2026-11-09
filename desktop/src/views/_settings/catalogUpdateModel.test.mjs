/**
 * Update from the catalog, tested where the rule lives. Run with
 * `node --test desktop/src/views/_settings/catalogUpdateModel.test.mjs`.
 */

import { strict as assert } from "node:assert";
import { test } from "node:test";

import { UPDATABLE_KINDS, rowAction, updateOutcome } from "./catalogUpdateModel.mjs";

test("a free id offers an install; an installed copy that matches offers nothing; one that differs offers an update; a channel never does", () => {
  assert.equal(rowAction({ kind: "agent", installed: false }), "install");
  assert.equal(rowAction({ kind: "agent", installed: true }), "installed", "no word from the node: nothing to compare");
  assert.equal(rowAction({ kind: "agent", installed: true, installed_differs: false }), "installed");
  assert.equal(rowAction({ kind: "agent", installed: true, installed_differs: null }), "installed");
  for (const kind of UPDATABLE_KINDS) {
    assert.equal(rowAction({ kind, installed: true, installed_differs: true }), "update", kind);
  }
  assert.equal(
    rowAction({ kind: "channel", installed: true, installed_differs: true }),
    "installed",
    "a channel is installed once and is then yours, whatever a row claimed",
  );
  assert.equal(rowAction({ kind: "skill", installed: false, installed_differs: true }), "install", "a free id installs, whatever else the row says");
});

test("an update's outcome: restated when the copy changed, created when only what it needed landed, unchanged when nothing did", () => {
  const nothing = { agents: [], skills: [], teams: [], channels: [], workflows: [], connectors: [], addons: [] };
  assert.equal(updateOutcome({ restated: true }, nothing), "restated");
  assert.equal(updateOutcome({ restated: true }, { ...nothing, skills: ["test-strategy"] }), "restated", "restated wins: the copy changed");
  assert.equal(updateOutcome({ restated: false }, { ...nothing, skills: ["test-strategy"] }), "created");
  assert.equal(updateOutcome({ restated: false }, nothing), "unchanged");
});
