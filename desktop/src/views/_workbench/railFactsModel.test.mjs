/**
 * What a workstream row says beside its name. Run with
 * `node --test desktop/src/views/_workbench/railFactsModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { factsWords, workstreamFacts } from "./railFactsModel.mjs";

const branch = { base: "main", ahead_of_base: 0, behind_base: 0, staged: 0, unstaged: 0, untracked: 0, clean: true };

test("a row wears its distance from the base, ahead then behind, with the sentence on hover", () => {
  assert.deepEqual(workstreamFacts(null), []);
  assert.deepEqual(workstreamFacts({ base: null, ahead_of_base: null, behind_base: null }), [], "the primary has no base");
  assert.deepEqual(workstreamFacts(branch), [], "level with its base");
  assert.deepEqual(workstreamFacts({ ...branch, ahead_of_base: 2 }), [{ text: "+2", title: "2 commits beyond main" }]);
  assert.deepEqual(workstreamFacts({ ...branch, behind_base: 1 }), [{ text: "−1", title: "1 commit on main not here" }]);
  assert.deepEqual(
    workstreamFacts({ ...branch, ahead_of_base: 3, behind_base: 1 }).map((f) => f.text),
    ["+3", "−1"],
  );
  assert.equal(workstreamFacts({ ...branch, base: null, ahead_of_base: 1 })[0].title, "1 commit beyond the base");
});

test("the tree's state is never a word on the row", () => {
  const dirty = { ...branch, clean: false, staged: 4, unstaged: 9, untracked: 12, conflicted: 1 };
  assert.deepEqual(workstreamFacts(dirty), []);
  const words = factsWords(workstreamFacts({ ...dirty, ahead_of_base: 2, behind_base: 5 }));
  assert.equal(words.text, "+2 · −5");
  assert.doesNotMatch(words.text + words.title, /dirty|staged|untracked|change/i);
});

test("the facts join as one string, and no facts is an empty one", () => {
  assert.deepEqual(factsWords([]), { text: "", title: "" });
  assert.deepEqual(factsWords([{ text: "+1", title: "a" }, { text: "−2", title: "b" }]), { text: "+1 · −2", title: "a · b" });
});
