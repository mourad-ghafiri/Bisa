/**
 * A file dropped from this machine: the facts in `dropModel.mjs`.
 * Run with `npm test` from `desktop/`.
 */

import { strict as assert } from "node:assert";
import { test } from "node:test";

import { isFileDrop, looseTargetRoot, pathsForDrop } from "./dropModel.mjs";

test("a drag is a file drag when the DOM says Files, whatever else it says", () => {
  assert.ok(isFileDrop(["Files"]));
  assert.ok(isFileDrop(["text/uri-list", "Files"]));
  assert.ok(!isFileDrop(["text/plain"]));
  assert.ok(!isFileDrop([]));
  assert.ok(!isFileDrop(null));
});

test("a drop's paths are the pasteboard's, paired to the DOM's names in the DOM's order, each once", () => {
  const paths = ["/Users/me/b.txt", "/Users/me/a.txt", "/tmp/a.txt"];
  assert.deepEqual(pathsForDrop(["a.txt", "b.txt"], paths), ["/Users/me/a.txt", "/Users/me/b.txt"]);
  // Two dropped files of one name take two pasteboard paths of that name, in the pasteboard's order.
  assert.deepEqual(pathsForDrop(["a.txt", "a.txt"], paths), ["/Users/me/a.txt", "/tmp/a.txt"]);
  // A name the pasteboard does not know is left out; nothing paired is nothing.
  assert.deepEqual(pathsForDrop(["c.txt", "a.txt"], paths), ["/Users/me/a.txt"]);
  assert.deepEqual(pathsForDrop(["c.txt"], paths), []);
  assert.deepEqual(pathsForDrop(["a.txt"], []), [], "another platform: the shell answers nothing");
  assert.deepEqual(pathsForDrop([], paths), []);
  assert.deepEqual(pathsForDrop(["a.txt"], ["a.txt", 7, null]), [], "only absolute paths count");
});

test("a loose file opens in the workbench on screen, else the most recent root, else nowhere", () => {
  assert.equal(looseTargetRoot({ name: "workbench", scope: "workstream", id: "01WS" }, ["goal:01G"]), "workstream:01WS");
  assert.equal(looseTargetRoot({ name: "inbox" }, ["workstream:01WS", "goal:01G"]), "workstream:01WS");
  assert.equal(looseTargetRoot({ name: "inbox" }, []), null);
  assert.equal(looseTargetRoot(null, ["nonsense"]), null, "a root key has a scope and an id");
});
