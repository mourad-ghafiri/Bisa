/**
 * What a memory gives back is what its parser knows, else nothing.
 * Run with `node --test desktop/src/shell/viewValuesModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { MAX_TEXT, MAX_WORDS, countValue, flagValue, idValue, sameKept, textValue, wordOf, wordsOf, wordsValue } from "./viewValuesModel.mjs";

test("a text is a text, cut at the longest; anything else is nothing", () => {
  assert.equal(textValue("ship"), "ship");
  assert.equal(textValue(""), "", "an empty box is a box");
  assert.equal(textValue("x".repeat(MAX_TEXT + 9)).length, MAX_TEXT);
  for (const raw of [null, undefined, 3, true, ["a"], { a: 1 }]) assert.equal(textValue(raw), undefined);
});

test("a switch is true or false and no other word", () => {
  assert.equal(flagValue(true), true);
  assert.equal(flagValue(false), false);
  for (const raw of ["true", 1, 0, null, undefined]) assert.equal(flagValue(raw), undefined);
});

test("a count is a whole number that is not negative", () => {
  assert.equal(countValue(0), 0);
  assert.equal(countValue(120), 120);
  for (const raw of [-1, 1.5, Number.NaN, Number.POSITIVE_INFINITY, "3", null]) assert.equal(countValue(raw), undefined);
});

test("a word is one of the vocabulary's, and a word off the list is nothing", () => {
  const tab = wordOf(["progress", "canvas"]);
  assert.equal(tab("canvas"), "canvas");
  assert.equal(tab("board"), undefined, "a vocabulary that changed selects nothing it no longer has");
  assert.equal(tab(1), undefined);
});

test("an id is a text with something in it", () => {
  assert.equal(idValue("01G"), "01G");
  for (const raw of ["", "x".repeat(MAX_TEXT + 1), 7, null]) assert.equal(idValue(raw), undefined);
});

test("a list keeps each word once, in order, and leaves out what is no word", () => {
  assert.deepEqual(wordsValue(["b", "a", "b", "", 3, null, "c"]), ["b", "a", "c"]);
  assert.deepEqual(wordsValue([]), []);
  assert.equal(wordsValue("a"), undefined);
  assert.equal(wordsValue({ 0: "a" }), undefined);
  assert.equal(wordsValue(Array.from({ length: MAX_WORDS + 20 }, (_, i) => `w${i}`)).length, MAX_WORDS);
});

test("a set is kept as its words, and reads back as the same set", () => {
  const opened = new Set(["b", "a"]);
  assert.deepEqual(wordsOf(opened), ["b", "a"]);
  assert.deepEqual(new Set(wordsValue(wordsOf(opened))), opened);
});

test("two values are the same when they are kept as the same text", () => {
  assert.equal(sameKept([], []), true);
  assert.equal(sameKept({ a: 1 }, { a: 1 }), true);
  assert.equal(sameKept("", null), false);
  assert.equal(sameKept(["a"], ["b"]), false);
  const loop = {};
  loop.self = loop;
  assert.equal(sameKept(loop, loop), false, "what cannot be kept is the same as nothing");
});
