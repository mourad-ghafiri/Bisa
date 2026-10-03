/**
 * The two sides of a conflict in the person's words. Run with
 * `node --test desktop/src/views/_work/conflictSidesModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { SIDE_ICON, agentQuestion, gitSideOf, isSwapped, isWholeFileKind, kindChoices, kindWords, sideByGit, sidesOf, tookWords, whatIsAConflict } from "./conflictSidesModel.mjs";

const SHA = "0123456789abcdef0123456789abcdef01234567";
const merge = { kind: "merge", branch: "main", ours: { role: "branch", name: "main", commit: null, subject: null }, theirs: { role: "branch", name: "feature/login", commit: SHA, subject: "Add login" }, step: null };
const pull = { ...merge, theirs: { role: "upstream", name: "origin/main", commit: SHA, subject: "Fix" } };
const rebase = { kind: "rebase", branch: "feature", ours: { role: "branch", name: "main", commit: SHA, subject: "Bump" }, theirs: { role: "commit", name: null, commit: SHA, subject: "Add login" }, step: { done: 3, total: 7 } };

test("mine is git's ours — except under a rebase, where the person's commit is git's theirs", () => {
  assert.equal(gitSideOf("merge", "mine"), "ours");
  assert.equal(gitSideOf("merge", "theirs"), "theirs");
  assert.equal(gitSideOf("rebase", "mine"), "theirs");
  assert.equal(gitSideOf("rebase", "theirs"), "ours");
  assert.equal(gitSideOf(null, "mine"), "ours");
  assert.ok(isSwapped("rebase") && !isSwapped("cherry_pick"));
});

test("the two sides are identities, not states: no colour of their own, told apart by a mark and a name", () => {
  const s = sidesOf("merge", merge);
  assert.notEqual(SIDE_ICON.mine, SIDE_ICON.theirs, "each side wears its own mark");
  assert.ok(!("tone" in s.mine) && !("tone" in s.theirs), "no side carries a tone — the accent is the summons, ok is done");
  assert.equal(sidesOf("rebase", rebase).mine.icon, SIDE_ICON.mine, "the mark follows the person's side through the swap");
});

test("a merge names your branch and the branch merged in — a pull the remote — and says what is happening", () => {
  const s = sidesOf("merge", merge);
  assert.deepEqual([s.mine.name, s.mine.git, s.mine.icon], ["main", "ours", SIDE_ICON.mine]);
  assert.deepEqual([s.theirs.name, s.theirs.git, s.theirs.icon], ["feature/login", "theirs", SIDE_ICON.theirs]);
  assert.match(s.mine.role, /your branch/);
  assert.match(s.theirs.role, /incoming/);
  assert.equal(s.title, "Merging feature/login into main");
  assert.match(s.explain, /^Merging feature\/login into main\./);
  assert.match(s.explain, /already merged/);
  assert.equal(s.step, null);
  assert.match(sidesOf("merge", pull).theirs.role, /the remote/);
  assert.equal(sidesOf("merge", pull).theirs.name, "origin/main");
  assert.equal(sideByGit(s, "theirs").name, "feature/login");
});

test("a rebase turns the sides around: mine is your commit, theirs the branch already there, with the step", () => {
  const s = sidesOf("rebase", rebase);
  assert.equal(s.mine.name, 'your commit "Add login"');
  assert.equal(s.mine.git, "theirs", "git's theirs is the commit replayed");
  assert.match(s.mine.role, /from feature/);
  assert.deepEqual([s.theirs.name, s.theirs.git], ["main", "ours"]);
  assert.match(s.theirs.role, /already there/);
  assert.equal(s.step, "commit 3 of 7");
  assert.equal(s.title, "Rebasing feature onto main");
  assert.match(s.explain, /replays your commits one at a time on top of main, and stopped on commit 3 of 7/);
  const bare = sidesOf("rebase", { ...rebase, theirs: { role: "commit", name: null, commit: SHA, subject: null }, ours: { role: "branch", name: null, commit: null, subject: null }, branch: null });
  assert.equal(bare.mine.name, "your commit 0123456", "no subject: the short sha");
  assert.equal(bare.theirs.name, "the branch you are rebasing onto");
});

test("a pick and a revert name the commit; no facts at all still gives honest words", () => {
  const pick = sidesOf("cherry_pick", { kind: "cherry_pick", branch: "main", ours: { role: "branch", name: "main", commit: null, subject: null }, theirs: { role: "commit", name: null, commit: SHA, subject: "Hotfix" }, step: { done: 1, total: 2 } });
  assert.equal(pick.theirs.name, 'the commit "Hotfix"');
  assert.equal(pick.mine.name, "main");
  assert.equal(pick.title, 'Cherry-picking the commit "Hotfix" onto main');
  assert.equal(pick.step, "commit 1 of 2");
  const revert = sidesOf("revert", { kind: "revert", branch: "main", ours: { role: "branch", name: "main", commit: null, subject: null }, theirs: { role: "commit", name: null, commit: SHA, subject: "Bad" }, step: null });
  assert.equal(revert.theirs.name, 'undoing "Bad"');
  assert.equal(revert.title, 'Reverting "Bad" on main');
  const none = sidesOf("merge", null);
  assert.deepEqual([none.mine.name, none.theirs.name], ["your branch", "the other side"]);
  assert.equal(sidesOf("rebase", merge).mine.git, "theirs", "facts for another operation are not read; the swap still holds");
});

test("each kind of conflict has words naming the sides, and the ones that cannot be merged block by block offer two whole choices", () => {
  const s = sidesOf("merge", merge);
  assert.deepEqual(kindWords("both_modified", s), { short: "both changed", sentence: "main and feature/login both changed this file." });
  assert.equal(kindWords("deleted_by_them", s).short, "deleted on feature/login");
  assert.equal(kindWords("deleted_by_us", s).sentence, "main deleted this file; feature/login changed it.");
  assert.equal(kindWords("both_added", s).short, "added by both");
  assert.equal(kindWords(null, s).short, "both changed");
  assert.equal(kindWords("deleted_by_us", sidesOf("rebase", rebase)).short, "deleted on main", "git's us under a rebase is the branch rebased onto");
  assert.ok(isWholeFileKind("deleted_by_them") && !isWholeFileKind("both_modified"));

  const them = kindChoices("deleted_by_them", s);
  assert.deepEqual(them.map((c) => [c.id, c.take, c.danger]), [["keep_mine", "ours", false], ["delete", "delete", true]]);
  assert.equal(them[0].label, "Keep mine — main");
  assert.equal(them[1].label, "Delete it, as feature/login did");
  const us = kindChoices("deleted_by_us", s);
  assert.deepEqual(us.map((c) => [c.id, c.take]), [["keep_theirs", "theirs"], ["delete", "delete"]]);
  assert.deepEqual(kindChoices("both_added", s).map((c) => [c.id, c.take]), [["keep_mine", "ours"], ["keep_theirs", "theirs"]]);
  assert.deepEqual(kindChoices("both_modified", sidesOf("rebase", rebase)).map((c) => [c.id, c.take]), [["keep_mine", "theirs"], ["keep_theirs", "ours"]], "a binary under a rebase: mine is git's theirs");
  assert.equal(kindChoices("both_deleted", s).length, 1);
});

test("the explainer, the toast after a take and the question for an agent are in the person's words", () => {
  const w = whatIsAConflict("merge");
  assert.equal(w.length, 3);
  assert.match(w[0], /cannot tell which version you want/);
  assert.match(w[2], /Abort puts the branch back .* before the merge started/);
  assert.match(whatIsAConflict("rebase")[1], /your commit already landed/);
  const s = sidesOf("merge", merge);
  assert.equal(tookWords("a.rs", "ours", s), "a.rs: kept mine — main — and staged it.");
  // Under a rebase git's `theirs` is the commit replayed — the person's own work — so a take of it kept *mine*.
  assert.equal(tookWords("a.rs", "theirs", sidesOf("rebase", rebase)), 'a.rs: kept mine — your commit "Add login" — and staged it.');
  assert.equal(tookWords("a.rs", "ours", sidesOf("rebase", rebase)), "a.rs: kept theirs — main — and staged it.");
  assert.equal(tookWords("a.rs", "delete", s), "a.rs deleted and settled.");
  const q = agentQuestion("src/app.ts", { ours: "port = 3000\n", theirs: "port = env\n", base: "port = 80\n" }, s, { at: 2, of: 3 });
  assert.match(q.text, /^Conflict 2 of 3 in src\/app\.ts\n/);
  assert.match(q.text, /--- mine: main \(your branch — what you have here\)\nport = 3000\n--- theirs: feature\/login/);
  assert.match(q.text, /--- base: what both started from\nport = 80$/);
  assert.match(q.question, /Do not stage or continue anything/);
  const r = agentQuestion("a", { ours: "onto\n", theirs: "commit\n", base: null }, sidesOf("rebase", rebase), { at: 1, of: 1 });
  assert.match(r.text, /--- mine: your commit "Add login"[^\n]*\ncommit\n--- theirs: main/, "under a rebase mine is git's theirs");
  assert.ok(!r.text.includes("--- base"));
});
