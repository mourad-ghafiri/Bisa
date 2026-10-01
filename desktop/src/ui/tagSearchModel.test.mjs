/**
 * One narrowing rule for every tagged, searchable list. Run with
 * `node --test desktop/src/ui/tagSearchModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { NO_TAG_FILTER, filterByTagsAndWords, matchesWords, parseTagFilter, passesTagFilter, searchNeedle } from "./tagSearchModel.mjs";

const items = [
  { name: "Builder", id: "builder", description: "Writes the code", tags: ["code", "engineering"] },
  { name: "Scribe", id: "scribe", description: null, tags: ["writing"] },
  { name: "Reviewer", id: "reviewer", description: "Reads a change", tags: [] },
  { name: "Untagged", id: "u", description: undefined, tags: undefined },
];

test("the tag filter: none selected passes all; any wants one of them, all wants every one; missing tags are no tags", () => {
  assert.ok(passesTagFilter(["a"], NO_TAG_FILTER) && passesTagFilter([], NO_TAG_FILTER) && passesTagFilter(undefined, NO_TAG_FILTER));
  assert.ok(passesTagFilter(["code", "web"], { selected: ["code", "ops"], match: "any" }));
  assert.ok(!passesTagFilter(["code", "web"], { selected: ["code", "ops"], match: "all" }));
  assert.ok(passesTagFilter(["code", "ops", "web"], { selected: ["code", "ops"], match: "all" }));
  assert.ok(!passesTagFilter(null, { selected: ["code"], match: "any" }), "no tags never passes a selection");
  assert.ok(passesTagFilter(null, { selected: [], match: "all" }), "an empty selection under all passes too");
});

test("the needle is trimmed and folded; an empty needle matches everything; an absent field is skipped, never a throw", () => {
  assert.equal(searchNeedle("  Build "), "build");
  assert.equal(searchNeedle(null), "");
  assert.equal(searchNeedle(undefined), "");
  assert.ok(matchesWords("", [null]) && matchesWords("", []));
  assert.ok(matchesWords("code", ["Builder", null, "Writes the CODE"]));
  assert.ok(!matchesWords("code", ["Scribe", undefined, null]));
  assert.ok(!matchesWords("x", []));
});

test("both controls compose in the list's order: tags first, then the words within what the tags left", () => {
  const names = (filter, q) => filterByTagsAndWords(items, filter, q, (i) => i.tags, (i) => [i.name, i.id, i.description]).map((i) => i.name);
  assert.deepEqual(names(NO_TAG_FILTER, ""), ["Builder", "Scribe", "Reviewer", "Untagged"]);
  assert.deepEqual(names(NO_TAG_FILTER, "  R  "), ["Builder", "Scribe", "Reviewer"], "the words search every field, folded and trimmed");
  assert.deepEqual(names(NO_TAG_FILTER, "u"), ["Builder", "Untagged"], "the id counts: Builder by its description, Untagged by its id");
  assert.deepEqual(names({ selected: ["code"], match: "any" }, ""), ["Builder"]);
  assert.deepEqual(names({ selected: ["code"], match: "any" }, "scribe"), [], "the search narrows within what the tags left, never widens");
  assert.deepEqual(names({ selected: ["writing"], match: "any" }, "SCRIBE"), ["Scribe"]);
  assert.deepEqual(names(NO_TAG_FILTER, "reads a"), ["Reviewer"], "a phrase matches within one field");
});

test("a filter read back from a memory selects each tag once and matches any unless it says all; what is no filter is nothing", () => {
  assert.deepEqual(parseTagFilter({ selected: ["ops", "quality", "ops"], match: "all" }), { selected: ["ops", "quality"], match: "all" });
  assert.deepEqual(parseTagFilter({ selected: ["ops", 3, "", null], match: "every" }), { selected: ["ops"], match: "any" });
  assert.deepEqual(parseTagFilter({ selected: [] }), { selected: [], match: "any" });
  assert.deepEqual(parseTagFilter(JSON.parse(JSON.stringify(NO_TAG_FILTER))), { selected: [], match: "any" });
  for (const raw of [null, undefined, "ops", ["ops"], {}, { selected: "ops" }]) assert.equal(parseTagFilter(raw), undefined);
});
