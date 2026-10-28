import { strict as assert } from "node:assert";
import { test } from "node:test";
import { canDiffHead, headDiffWords, headSides } from "./headDiffModel.mjs";

test("only a root file of a checkout, on disk, can be diffed against HEAD — blame's rule, plus on disk", () => {
  assert.equal(canDiffHead({ scope: "workstream", isFile: true, onDisk: true }), true);
  assert.equal(canDiffHead({ scope: "goal", isFile: true, onDisk: true }), false, "a goal's folder has no repository to ask");
  assert.equal(canDiffHead({ scope: "work_item", isFile: true, onDisk: true }), false);
  assert.equal(canDiffHead({ scope: "workstream", isFile: false, onDisk: true }), false, "a loose file is no path git knows");
  assert.equal(canDiffHead({ scope: "workstream", isFile: false, onDisk: false }), false, "an untitled document has nothing on disk");
});

test("the sides read as HEAD's text, a file HEAD lacks, a side that is not text, or one cut at the cap", () => {
  assert.deepEqual(headSides({ original: "a\n", modified: "b\n", binary: false, truncated: false }), { kind: "ready", original: "a\n" });
  assert.deepEqual(headSides({ original: null, modified: "b\n", binary: false, truncated: false }), { kind: "new", original: "" }, "not in HEAD: every line is yours");
  assert.deepEqual(headSides({ modified: "b\n", binary: false, truncated: false }), { kind: "new", original: "" }, "an absent side is a missing one");
  assert.deepEqual(headSides({ original: "a\n", modified: null, binary: true, truncated: false }), { kind: "binary" }, "a side that is not text cannot be diffed");
  assert.deepEqual(headSides({ original: null, binary: true, truncated: true }), { kind: "binary" }, "binary outranks every other verdict");
  assert.deepEqual(headSides({ original: "a\n", modified: "b\n", binary: false, truncated: true }), { kind: "truncated", original: "a\n" }, "a cut side is drawn and said so");
  assert.deepEqual(headSides({ original: "", binary: false, truncated: false }), { kind: "ready", original: "" }, "an empty file in HEAD is a text, not a missing one");
});

test("the bar's words name the verdict", () => {
  assert.equal(headDiffWords("ready"), "HEAD · yours (editable)");
  assert.equal(headDiffWords("new"), "Not in HEAD — every line is yours");
  assert.equal(headDiffWords("binary"), "HEAD's side of this file is not text, so there is nothing to diff");
  assert.equal(headDiffWords("truncated"), "HEAD's side was cut at the cap — the diff is not whole");
});
