/**
 * Find and replace as one fact. Run with
 * `node --test desktop/src/ui/find/findModel.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";

import { compileFind, countWords, emptyFind, hasQuery, matchesOf, replaceAll, replaceOne, stepIndex } from "./findModel.mjs";

const find = (over) => ({ ...emptyFind(), ...over });

test("a literal query finds itself, case-blind unless the case matters; an empty query finds nothing", () => {
  assert.deepEqual(matchesOf("Foo foo FOO", find({ query: "foo" })), [
    { start: 0, end: 3 },
    { start: 4, end: 7 },
    { start: 8, end: 11 },
  ]);
  assert.deepEqual(matchesOf("Foo foo FOO", find({ query: "foo", caseSensitive: true })), [{ start: 4, end: 7 }]);
  assert.deepEqual(matchesOf("a.b", find({ query: "." })), [{ start: 1, end: 2 }], "a literal dot is a dot");
  assert.deepEqual(matchesOf("anything", emptyFind()), []);
  assert.equal(hasQuery(emptyFind()), false);
  assert.equal(hasQuery(find({ query: "x" })), true);
  assert.equal(compileFind(emptyFind()), null);
});

test("a regex query is an expression, and one the engine refuses is null — never a throw", () => {
  assert.deepEqual(matchesOf("a1 b22 c", find({ query: "[a-z]\\d+", regex: true })), [
    { start: 0, end: 2 },
    { start: 3, end: 6 },
  ]);
  assert.equal(compileFind(find({ query: "[", regex: true })), null, "half-typed");
  assert.deepEqual(matchesOf("abc", find({ query: "[", regex: true })), []);
  assert.deepEqual(matchesOf("ab", find({ query: "x*", regex: true })).length, 3, "an empty match advances and ends");
});

test("replace all swaps every match and counts them; a literal replacement keeps its dollars, a regex one reads its groups", () => {
  assert.deepEqual(replaceAll("foo Foo", find({ query: "foo", replacement: "bar" })), { text: "bar bar", count: 2 });
  assert.deepEqual(replaceAll("cost", find({ query: "cost", replacement: "$5" })), { text: "$5", count: 1 });
  assert.deepEqual(replaceAll("john smith", find({ query: "(\\w+) (\\w+)", regex: true, replacement: "$2, $1" })), { text: "smith, john", count: 1 });
  assert.deepEqual(replaceAll("nothing", find({ query: "zzz", replacement: "y" })), { text: "nothing", count: 0 });
});

test("replace one swaps the match at the index — its own groups — and leaves the text alone past the last", () => {
  assert.deepEqual(replaceOne("a1 a2 a3", find({ query: "a(\\d)", regex: true, replacement: "b$1" }), 1), { text: "a1 b2 a3", replaced: true });
  assert.deepEqual(replaceOne("x x", find({ query: "x", replacement: "y" }), 0), { text: "y x", replaced: true });
  assert.deepEqual(replaceOne("x x", find({ query: "x", replacement: "y" }), 5), { text: "x x", replaced: false });
  assert.deepEqual(replaceOne("x", find({ query: "x", replacement: "$" }), 0), { text: "$", replaced: true });
});

test("stepping wraps both ways and lands nowhere with no matches", () => {
  assert.equal(stepIndex(0, 3, "next"), 1);
  assert.equal(stepIndex(2, 3, "next"), 0);
  assert.equal(stepIndex(0, 3, "previous"), 2);
  assert.equal(stepIndex(-1, 3, "next"), 0, "from nothing, the first");
  assert.equal(stepIndex(-1, 3, "previous"), 2, "from nothing, backwards, the last");
  assert.equal(stepIndex(0, 0, "next"), -1);
  assert.equal(countWords(0, null), "");
  assert.equal(countWords(-1, 0), "none");
  assert.equal(countWords(2, 7), "3/7");
});
