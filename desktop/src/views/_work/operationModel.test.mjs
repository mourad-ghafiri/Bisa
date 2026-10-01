/**
 * The Resolve card's facts. Run with
 * `node --test desktop/src/views/_work/operationModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { abortConsent, conflictedPaths, continueConsent, continuedWords, nextConflict, operationVerbs, operationWord, operationWords, skipConsent, skippedWords } from "./operationModel.mjs";

const SHA = "0123456789abcdef0123456789abcdef01234567";
const facts = { kind: "merge", branch: "main", ours: { role: "branch", name: "main", commit: null, subject: null }, theirs: { role: "branch", name: "feature", commit: SHA, subject: "theirs" }, step: null };
const rebaseFacts = { kind: "rebase", branch: "topic", ours: { role: "branch", name: "main", commit: SHA, subject: null }, theirs: { role: "commit", name: null, commit: SHA, subject: "Add login" }, step: { done: 1, total: 2 } };
const op = { kind: "merge", from: "feature", to: "main", commits: [], paths: ["a.rs", "b.rs", "c.rs"] };
const row = (path, conflict) => ({ path, conflicted: true, conflict, index: "U", worktree: "U", staged: false, unstaged: false, untracked: false });

test("the card names the operation from the node's facts, lists the conflicted files with their kinds, and counts what is settled of what the 409 named", () => {
  const w = operationWords("merge", facts, op, [row("b.rs", "both_modified"), { path: "x.rs", conflicted: false, staged: true }]);
  assert.equal(w.title, "Merging feature into main");
  assert.match(w.explain, /^Merging feature into main\./);
  assert.equal(w.step, null);
  assert.equal(w.line, "1 file still conflicted · 2 of 3 settled.");
  assert.deepEqual(w.files, ["b.rs"]);
  assert.deepEqual(
    w.checklist.map((r) => [r.path, r.short, r.settled]),
    [["b.rs", "both changed", false], ["a.rs", "settled", true], ["c.rs", "settled", true]],
  );
  assert.deepEqual([w.settled, w.total], [2, 3]);
  assert.equal(w.sides.theirs.name, "feature");
  assert.equal(operationWords("merge", facts, op, []).line, "Every file is settled — continue when ready.");
  const r = operationWords("rebase", rebaseFacts, null, [row("x", "deleted_by_us"), row("y", null)]);
  assert.equal(r.title, "Rebasing topic onto main", "started elsewhere: the facts still name it");
  assert.equal(r.step, "commit 1 of 2");
  assert.equal(r.line, "2 files still conflicted.");
  assert.deepEqual(r.checklist.map((c) => c.short), ["deleted on main", "both changed"], "git's us under a rebase is the branch rebased onto");
  assert.deepEqual([r.settled, r.total], [0, 2]);
  assert.equal(operationWords("rebase", null, op, [row("x", null)]).title, "Rebasing your branch onto the branch you are rebasing onto", "no facts: honest words, and a merge's paths say nothing about a rebase");
  assert.deepEqual(conflictedPaths([{ path: "a", conflicted: true }, { path: "b", conflicted: false }, null]), ["a"]);
  assert.equal(operationWord("cherry_pick"), "cherry-pick");
});

test("Continue waits for the files, Skip is not a merge's, and everything waits while another operation runs", () => {
  const v = operationVerbs("rebase", ["a.rs", "b.rs"], false);
  assert.ok(v.continue.disabled);
  assert.equal(v.continue.reason, "2 files still conflicted — settle them first.");
  assert.equal(operationVerbs("rebase", ["a.rs"], false).continue.reason, "1 file still conflicted — settle it first.");
  assert.ok(!v.skip.disabled && v.skip.label === "Skip this commit");
  assert.equal(v.abort.label, "Abort the rebase");
  assert.ok(!operationVerbs("rebase", [], false).continue.disabled, "settled: Continue is lit");
  assert.equal(operationVerbs("merge", [], false).skip, null, "a merge has nothing to skip");
  const busy = operationVerbs("cherry_pick", [], true);
  assert.ok(busy.continue.disabled && busy.skip.disabled && busy.abort.disabled);
  assert.match(busy.abort.reason, /running/);
});

test("the confirmations are questions with the verb as the button, say what the next step makes, and promise nothing is lost", () => {
  const c = continueConsent("merge", facts);
  assert.equal(c.title, "Continue the merge?");
  assert.match(c.body, /Merge branch 'feature'/);
  assert.match(c.body, /No editor opens/);
  assert.equal(c.confirm, "Continue");
  assert.ok(!c.danger);
  assert.ok(!continueConsent("merge", null).body.includes("Merge branch"), "no facts: no invented name");
  assert.match(continueConsent("rebase", null).body, /replayed/);
  assert.equal(skipConsent("cherry_pick").title, "Skip this commit?");
  assert.ok(skipConsent("cherry_pick").danger);
  assert.equal(abortConsent("revert").title, "Abort the revert?");
  assert.equal(abortConsent("revert").confirm, "Abort");
  assert.match(abortConsent("merge").body, /before the merge started/);
  assert.match(abortConsent("merge").body, /Nothing is lost/);
  assert.equal(continuedWords("merge", false), "Continued the merge — done.");
  assert.equal(continuedWords("rebase", true), "The rebase went on and stopped again.");
  assert.equal(skippedWords("cherry_pick", false), "Skipped the commit — the cherry-pick is done.");
  assert.equal(skippedWords("rebase", true), "Skipped the commit; the rebase stopped again.");
});

test("the next conflict is the one after the settled path, wrapping to the first, and none when all are settled", () => {
  assert.equal(nextConflict(["a", "b", "c"], "a"), "b");
  assert.equal(nextConflict(["a", "b", "c"], "c"), "a");
  assert.equal(nextConflict(["b", "c"], null), "b");
  assert.equal(nextConflict(["a"], "a"), null);
  assert.equal(nextConflict([], "a"), null);
});
