/**
 * A conflicted file, settled the way a person settles it (ide/04 §Conflicts):
 * the markers git wrote read into blocks, the sides are named from the
 * operation's facts — *mine* is your commit under a rebase — a choice per
 * block moves the progress, the next-unsettled key walks and wraps, the file
 * composes from the choices with the swap honoured, and every word a toast or
 * an agent's question says is the person's. No DOM.
 *
 * Run with `node --test desktop/src/scenarios/conflicts.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { choose, chooseAll, compose, hasMarkers, nextUnsettled, parseConflicts, previousUnsettled, progress, unsettledIds } from "../views/_work/conflictBlocksModel.mjs";
import { agentQuestion, gitSideOf, isSwapped, sideByGit, sidesOf, tookWords } from "../views/_work/conflictSidesModel.mjs";

const FILE = ["a\n", "<<<<<<< HEAD\n", "mine\n", "=======\n", "theirs\n", ">>>>>>> feature/login\n", "b\n", "<<<<<<< HEAD\n", "m2\n", "=======\n", "t2\n", ">>>>>>> feature/login\n"].join("");
const side = (role, name, over = {}) => ({ role, name, commit: null, subject: null, ...over });
const mergeFacts = { kind: "merge", branch: "main", ours: side("branch", "main"), theirs: side("branch", "feature/login"), step: null };
const rebaseFacts = { kind: "rebase", branch: "feature/login", ours: side("branch", "main"), theirs: side("commit", null, { commit: "0123456789abcdef", subject: "Add login" }), step: { done: 2, total: 5 } };

test("a merge: the blocks read, the sides are the branches, a choice per block settles the file, and the composed text is what git would keep", () => {
  assert.ok(hasMarkers(FILE));
  const { segments, problem } = parseConflicts(FILE);
  assert.equal(problem, null);
  const sides = sidesOf("merge", mergeFacts);
  assert.equal(isSwapped("merge"), false);
  assert.deepEqual([sides.mine.name, sides.theirs.name], ["main", "feature/login"]);
  assert.equal(sides.title, "Merging feature/login into main");

  // Nothing chosen: the file as git left it, two to settle, the first is next.
  let choices = {};
  assert.deepEqual(progress(segments, choices), { settled: 0, total: 2 });
  assert.equal(compose(segments, choices, false), FILE);
  assert.equal(nextUnsettled(segments, choices, null), "c1");

  // Keep mine on the first: one settled, the second is next, the first block reads "mine".
  choices = choose(choices, "c1", "mine");
  assert.deepEqual(progress(segments, choices), { settled: 1, total: 2 });
  assert.deepEqual(unsettledIds(segments, choices), ["c2"]);
  assert.equal(nextUnsettled(segments, choices, "c1"), "c2");
  assert.equal(previousUnsettled(segments, choices, "c1"), "c2", "wrapping backwards lands on the only open one");
  // Keep theirs on the second: settled, and the file is what git would keep.
  choices = choose(choices, "c2", "theirs");
  assert.deepEqual(progress(segments, choices), { settled: 2, total: 2 });
  assert.equal(compose(segments, choices, false), "a\nmine\nb\nt2\n");
  assert.equal(nextUnsettled(segments, choices, "c2"), "c1", "all settled: any block, in order");
  // Keep all mine from the start: one act settles both.
  assert.equal(compose(segments, chooseAll(segments, "mine"), false), "a\nmine\nb\nm2\n");
  assert.equal(compose(segments, chooseAll(segments, "theirs"), false), "a\ntheirs\nb\nt2\n");
});

test("a rebase turns the sides around and every take, toast and question reads the mapping: mine is your commit, theirs the branch you are rebasing onto", () => {
  const { segments } = parseConflicts(FILE);
  const sides = sidesOf("rebase", rebaseFacts);
  assert.equal(isSwapped("rebase"), true);
  assert.equal(gitSideOf("rebase", "mine"), "theirs", "git's theirs is the commit replayed — yours");
  assert.equal(sides.mine.name, 'your commit "Add login"');
  assert.equal(sides.theirs.name, "main");
  assert.equal(sides.step, "commit 2 of 5");
  assert.equal(sideByGit(sides, "ours").key, "theirs", "git's ours is the branch already there");
  // The blocks are git's HEAD/incoming order; a choice of *mine* under the swap keeps the incoming text.
  assert.equal(compose(segments, chooseAll(segments, "mine"), true), "a\ntheirs\nb\nt2\n", "mine is git's theirs under a rebase");
  assert.equal(compose(segments, chooseAll(segments, "theirs"), true), "a\nmine\nb\nm2\n");
  // The toast after a whole-file take names the side in the person's words, from git's word.
  assert.equal(tookWords("a.rs", "theirs", sides), 'a.rs: kept mine — your commit "Add login" — and staged it.');
  assert.equal(tookWords("a.rs", "ours", sides), "a.rs: kept theirs — main — and staged it.");
  assert.equal(tookWords("a.rs", "delete", sides), "a.rs deleted and settled.");
  // The question for an agent labels each side the way the person sees it.
  const q = agentQuestion("src/app.ts", { ours: "port = 3000\n", theirs: "port = env\n", base: null }, sides, { at: 1, of: 2 });
  assert.match(q.text, /^Conflict 1 of 2 in src\/app\.ts\n/);
  assert.match(q.text, /--- mine: your commit "Add login"/);
  assert.match(q.text, /--- theirs: main/);
  // Facts for another operation are stale and not read; the swap still holds.
  assert.equal(sidesOf("rebase", mergeFacts).mine.git, "theirs");
});
