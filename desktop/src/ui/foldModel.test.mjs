import { strict as assert } from "node:assert";
import { test } from "node:test";

import { FOLD_MAX, firstSentence, foldLabel, needsFold } from "./foldModel.mjs";

test("the first sentence is the summary, marked when there is more", () => {
  assert.equal(firstSentence("Looks good. One nit on line 3."), "Looks good.…");
  assert.equal(firstSentence("Looks good."), "Looks good.");
  assert.equal(firstSentence("Rename this\nand that"), "Rename this…", "a line break ends the summary too");
  assert.equal(firstSentence("Is this intended? It reads odd."), "Is this intended?…");
  assert.equal(firstSentence(""), "");
  assert.equal(firstSentence(null), "");
  assert.equal(firstSentence("   \n  "), "");
});

test("a long first sentence is cut at a word with an ellipsis", () => {
  const long = `${"word ".repeat(60)}end.`;
  const cut = firstSentence(long);
  assert.ok(cut.length <= FOLD_MAX + 1, `${cut.length}`);
  assert.ok(cut.endsWith("…"));
  assert.ok(!cut.endsWith(" …"), "no trailing space before the mark");
  assert.equal(firstSentence("short one", 40), "short one");
  assert.equal(firstSentence("a-very-long-hyphenated-token-with-no-spaces-anywhere-in-it-at-all-really", 20), "a-very-long-hyphenat…", "no word boundary: a hard cut");
});

test("markdown marks are not part of a summary", () => {
  assert.equal(firstSentence("**Blocking:** the `total()` call rounds twice. See below."), "Blocking: the total() call rounds twice.…");
  assert.equal(firstSentence("- rename x\n- rename y"), "rename x…");
  assert.equal(firstSentence("## Summary\nAll good."), "Summary…");
  assert.equal(firstSentence("See [the docs](https://example.test/x)."), "See the docs.");
  assert.equal(firstSentence("```rs\nlet x = 1;\n```\nThen this."), "[code] Then this.");
});

test("needsFold says when folding hides anything", () => {
  assert.equal(needsFold("Looks good."), false);
  assert.equal(needsFold("Looks good. Ship it."), true);
  assert.equal(needsFold("one line\ntwo"), true);
  assert.equal(needsFold(`${"x".repeat(200)}`), true);
  assert.equal(needsFold(""), false);
  assert.equal(needsFold("**bold** only"), false, "marks alone do not make a fold");
});

test("the control's words", () => {
  assert.equal(foldLabel(false), "Show all");
  assert.equal(foldLabel(true), "Show less");
});
