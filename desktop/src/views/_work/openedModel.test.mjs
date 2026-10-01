/**
 * The rows a person opened on a list, as a screen keeps them. Run with
 * `node --test desktop/src/views/_work/openedModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { wordsOf } from "../../shell/viewValuesModel.mjs";
import { MAX_OPENED, parseOpened, toggled } from "./openedModel.mjs";

test("a row pressed is opened, and pressed again is closed", () => {
  const none = new Set();
  const one = toggled(none, "k1");
  assert.deepEqual([...one], ["k1"]);
  assert.deepEqual([...toggled(one, "k2")], ["k1", "k2"]);
  assert.deepEqual([...toggled(one, "k1")], []);
  assert.equal(none.size, 0, "the set handed in is never written to");
  assert.notEqual(toggled(one, "k1"), one, "a new set either way: what is drawn follows it");
});

test("what was opened reads back the same after the memory kept it as words", () => {
  const opened = toggled(toggled(new Set(), "k1"), "k2");
  const stored = JSON.parse(JSON.stringify(wordsOf(opened)));
  assert.deepEqual([...parseOpened(stored)], ["k1", "k2"]);
  assert.deepEqual([...parseOpened(["k1", "k1", 7, "", "k2"])], ["k1", "k2"], "each row once, and only what is a row");
  assert.deepEqual([...parseOpened([])], []);
});

test("what is no list of rows is nothing", () => {
  for (const bad of [null, undefined, "k1", 7, {}, { 0: "k1" }]) assert.equal(parseOpened(bad), undefined, JSON.stringify(bad));
});

test("a list keeps the rows opened last, never every row of a long afternoon", () => {
  let opened = new Set();
  for (let i = 0; i <= MAX_OPENED; i += 1) opened = toggled(opened, `k${i}`);
  assert.equal(opened.size, MAX_OPENED);
  assert.ok(!opened.has("k0"), "the row opened longest ago is closed");
  assert.ok(opened.has(`k${MAX_OPENED}`));
  assert.deepEqual([...toggled(new Set(["a", "b"]), "c", 2)], ["b", "c"]);
  // A memory written by another version may hold more: the newest are read.
  const many = Array.from({ length: MAX_OPENED + 3 }, (_, i) => `k${i}`);
  const read = parseOpened(many);
  assert.equal(read.size, MAX_OPENED);
  assert.ok(!read.has("k2") && read.has("k3") && read.has(`k${MAX_OPENED + 2}`));
});
