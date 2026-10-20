/**
 * Which editor is active, which line waits, and what the quit question counts.
 *
 * Run with `npm test` from `desktop/`.
 */

import { strict as assert } from "node:assert";
import { test } from "node:test";

import { MOUNT_GRACE_MS, actionRequested, actionTaken, INITIAL, activated, dirtyCount, lineRequested, lineTaken, released } from "./editorRegistryModel.mjs";

test("a mount claims the active slot until another is focused, and an unmount releases only its own claim", () => {
  let s = activated(INITIAL, "root|a");
  s = activated(s, "root|b");
  assert.equal(s.active, "root|b");
  assert.equal(released(s, "root|a").active, "root|b", "the other editor of a split keeps the slot");
  assert.equal(released(s, "root|b").active, null);
  assert.equal(activated(s, "root|b"), s, "re-focusing the active editor changes nothing");
});

test("a line asked before its editor mounts waits, once; the newer request replaces the older, for this document or another", () => {
  let s = lineRequested(INITIAL, "root|a", 12, false);
  s = lineRequested(s, "root|a", 40, false);
  assert.deepEqual(s.pending, { key: "root|a", line: 40 }, "asked again, the newer line wins");
  s = lineRequested(s, "root|b", 3, false);
  assert.equal(lineTaken(s, "root|a").line, null, "asked for another document, the older request is dropped");
  const taken = lineTaken(s, "root|b");
  assert.equal(taken.line, 3);
  assert.equal(lineTaken(taken.state, "root|b").line, null, "taken once");
  assert.deepEqual(lineRequested(s, "root|c", 7, true).pending, { key: null, line: null }, "shown at once, nothing waits");
});

test("the quit question counts unsaved documents and the dirty sources outside the workbench", () => {
  const dirty = { dirty: () => true };
  const clean = { dirty: () => false };
  assert.equal(dirtyCount([], []), 0);
  assert.equal(dirtyCount(["root|a", "root|b"], [dirty, clean]), 3);
});

test("an editor that leaves releases its own registration and no other's", async () => {
  const { readFileSync } = await import("node:fs");
  const registry = readFileSync(new URL("./editorRegistry.ts", import.meta.url), "utf8");
  const register = registry.slice(registry.indexOf("export function registerEditor"), registry.indexOf("/**", registry.indexOf("export function registerEditor")));
  assert.ok(register.includes("if (editors.get(key)?.handle !== handle) return;"), "a document moved to another pane keeps the handle its new editor registered");
  assert.ok(registry.includes("if (after.get(key) === source)"), "as a dirty source does");
});

test("an editor action asked before its editor mounts waits, once, as a line does: run now nothing waits, the newer replaces the older for this document or another, and a mount is given its grace", () => {
  const ran = actionRequested(INITIAL, "k1", "actions.find", true);
  assert.deepEqual(actionTaken(ran, "k1"), { action: null, state: ran });
  const held = actionRequested(INITIAL, "k1", "actions.find", false);
  const other = actionRequested(held, "k2", "editor.action.startFindReplaceAction", false);
  assert.deepEqual(actionTaken(other, "k1"), { action: null, state: other }, "the person moved on");
  const taken = actionTaken(other, "k2");
  assert.equal(taken.action, "editor.action.startFindReplaceAction");
  assert.deepEqual(actionTaken(taken.state, "k2"), { action: null, state: taken.state }, "taken once");
  assert.equal(MOUNT_GRACE_MS, 3000);
});
