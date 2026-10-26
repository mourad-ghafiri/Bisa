import { strict as assert } from "node:assert";
import { test } from "node:test";
import { addWord, fromList, hasWord, joinWords, removeWord, splitWords, toggleWord, wordsOr } from "./manyModel.mjs";

test("a list is written bare for one, as a list for several, and as nothing for none", () => {
  assert.equal(fromList([]), null);
  assert.equal(fromList(null), null);
  assert.equal(fromList(["commit"]), "commit");
  assert.deepEqual(fromList(["commit", "push"]), ["commit", "push"]);
  assert.equal(fromList(["", null, "a"]), "a", "blanks are not entries");
  assert.deepEqual(fromList([{ input: "repo" }]), { input: "repo" }, "a reference is a value");
});

test("a word is held, toggled, added once and removed, whatever the value's shape", () => {
  assert.ok(hasWord("commit", "commit"));
  assert.ok(hasWord(["commit", "push"], "push"));
  assert.ok(!hasWord(null, "push"));
  assert.ok(hasWord([{ input: "repo" }, "id"], { input: "repo" }), "a reference by its shape");
  assert.deepEqual(toggleWord("commit", "push"), ["commit", "push"]);
  assert.equal(toggleWord(["commit", "push"], "push"), "commit");
  assert.equal(toggleWord("commit", "commit"), null, "the last word off is nothing");
  assert.equal(addWord(null, " a "), "a", "trimmed");
  assert.equal(addWord("a", "a"), "a", "once");
  assert.equal(addWord("a", "  "), "a", "a blank adds nothing");
  assert.deepEqual(addWord("a", "b"), ["a", "b"]);
  assert.deepEqual(addWord("a", { input: "x" }), ["a", { input: "x" }]);
  assert.equal(removeWord(["a", "b"], "b"), "a");
  assert.equal(removeWord("a", "a"), null);
});

test("an absent closed-set field reads as its default, and a mixed list splits into words and the rest", () => {
  assert.deepEqual(wordsOr(null, "commit"), ["commit"]);
  assert.deepEqual(wordsOr(null, null), [], "a default of none is any");
  assert.deepEqual(wordsOr(["push"], "commit"), ["push"]);
  const words = ["you", "agents", "people", "anyone"];
  assert.deepEqual(splitWords(["agents", { agent: "herald" }, { input: "who" }, "people"], words), {
    words: ["agents", "people"],
    rest: [{ agent: "herald" }, { input: "who" }],
  });
  assert.deepEqual(splitWords("you", words), { words: ["you"], rest: [] });
  assert.deepEqual(splitWords(null, words), { words: [], rest: [] });
  assert.equal(joinWords([], []), null);
  assert.equal(joinWords(["you"], null), "you");
  assert.deepEqual(joinWords("agents", [{ agent: "herald" }]), ["agents", { agent: "herald" }]);
});
