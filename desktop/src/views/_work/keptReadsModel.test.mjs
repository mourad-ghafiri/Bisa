/**
 * A read kept for the window's life: keyed by what and of which thing,
 * bounded, and never an answer that is nothing.
 * Run with `node --test desktop/src/views/_work/keptReadsModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { KeptReads, MAX_READS, readKey } from "./keptReadsModel.mjs";

test("a key is what is read and of which thing; a part that is nothing makes no key", () => {
  assert.equal(readKey("workflows"), "workflows");
  assert.equal(readKey("goal", "01G"), "goal:01G");
  assert.equal(readKey("pulse", "goals", "head"), "pulse:goals:head");
  assert.equal(readKey("goal", null), null);
  assert.equal(readKey("goal", undefined), null);
  assert.equal(readKey("goal", ""), null);
  assert.equal(readKey("", "01G"), null);
});

test("an answer kept is read back by identity, and no key reads nothing", () => {
  const reads = new KeptReads();
  const goal = { id: "01G" };
  reads.keep("goal:01G", goal);
  assert.equal(reads.read("goal:01G"), goal);
  assert.equal(reads.read("goal:01H"), undefined);
  assert.equal(reads.read(null), undefined);
  assert.equal(reads.read(undefined), undefined);
});

test("an answer that is nothing keeps nothing, and leaves what was kept", () => {
  const reads = new KeptReads();
  reads.keep("goal:01G", { id: "01G" });
  reads.keep("goal:01G", null);
  reads.keep("goal:01G", undefined);
  reads.keep(null, { id: "x" });
  assert.deepEqual(reads.read("goal:01G"), { id: "01G" });
  assert.equal(reads.size, 1);
});

test("the newest used reads are kept and the oldest forgotten past the cap", () => {
  const reads = new KeptReads(2);
  reads.keep("a", 1);
  reads.keep("b", 2);
  assert.equal(reads.read("a"), 1, "a read is a use");
  reads.keep("c", 3);
  assert.equal(reads.read("b"), undefined, "the one not used");
  assert.equal(reads.read("a"), 1);
  assert.equal(reads.read("c"), 3);
  assert.ok(MAX_READS >= 32, "room for the screens a person moves between");
});

test("one read is forgotten, or all of them", () => {
  const reads = new KeptReads();
  reads.keep("a", 1);
  reads.keep("b", 2);
  reads.forget("a");
  reads.forget(null);
  assert.equal(reads.read("a"), undefined);
  assert.equal(reads.read("b"), 2);
  reads.clear();
  assert.equal(reads.size, 0);
});
