/**
 * A conflicted file as blocks, and the file the choices compose. Run with
 * `node --test desktop/src/views/_work/conflictBlocksModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { CHOICES, blockWords, choose, chooseAll, compose, conflictsOf, documentWords, foldWords, hasMarkers, lineCount, nextUnsettled, parseConflicts, previousUnsettled, progress, textFor, unchoose, unsettledIds } from "./conflictBlocksModel.mjs";

const MERGE = ["a\n", "<<<<<<< HEAD\n", "mine\n", "=======\n", "theirs\n", ">>>>>>> feature\n", "b\n", "c\n", "<<<<<<< HEAD\n", "m2\n", "=======\n", "t2\n", ">>>>>>> feature\n"].join("");
const DIFF3 = ["a\n", "<<<<<<< HEAD\n", "mine\n", "||||||| base\n", "orig\n", "=======\n", "theirs\n", ">>>>>>> feature\n"].join("");

test("the markers git writes — merge, diff3 and zdiff3 styles — read into text runs and conflicts, and join back to the file", () => {
  const { segments, problem } = parseConflicts(MERGE);
  assert.equal(problem, null);
  assert.deepEqual(
    segments.map((s) => s.kind),
    ["text", "conflict", "text", "conflict"],
  );
  assert.equal(segments[0].text, "a\n");
  assert.deepEqual([segments[1].id, segments[1].ours, segments[1].base, segments[1].theirs], ["c1", "mine\n", null, "theirs\n"]);
  assert.equal(segments[1].raw, "<<<<<<< HEAD\nmine\n=======\ntheirs\n>>>>>>> feature\n", "the conflict as git wrote it");
  assert.equal(segments[2].text, "b\nc\n");
  assert.equal(segments[3].id, "c2");
  assert.equal(compose(segments, {}, false), MERGE, "no choice: the file as it was");

  const d3 = parseConflicts(DIFF3).segments;
  assert.equal(d3[1].base, "orig\n", "the diff3 base is read");
  assert.equal(d3[1].theirs, "theirs\n");
  assert.equal(compose(d3, {}, false), DIFF3);

  assert.deepEqual(parseConflicts("").segments, []);
  assert.deepEqual(parseConflicts("plain\ntext"), { segments: [{ kind: "text", text: "plain\ntext" }], problem: null }, "a file with no markers is one run, no newline invented");
  const empty = parseConflicts("<<<<<<< HEAD\n=======\n>>>>>>> x\n").segments[0];
  assert.deepEqual([empty.ours, empty.theirs], ["", ""], "an empty side is an empty run");
});

test("a torn file — a marker out of place — is one run with a sentence, never a guess", () => {
  const torn = parseConflicts("a\n<<<<<<< HEAD\nmine\n=======\ntheirs\n");
  assert.equal(torn.segments.length, 1);
  assert.equal(torn.segments[0].text, "a\n<<<<<<< HEAD\nmine\n=======\ntheirs\n");
  assert.match(torn.problem, /never closed/);
  assert.match(parseConflicts("a\n=======\nb\n").problem, /no <<<<<<< before it/);
  assert.match(parseConflicts("<<<<<<< a\nx\n<<<<<<< b\n").problem, /inside a conflict/);
  assert.match(parseConflicts("<<<<<<< a\nx\n=======\ny\n||||||| b\n>>>>>>> c\n").problem, /after =======/);
  assert.ok(hasMarkers("x\n<<<<<<< HEAD\n"));
  assert.ok(hasMarkers("=======\n") && hasMarkers(">>>>>>> x") && hasMarkers("||||||| base"));
  assert.ok(!hasMarkers("a <<<<<<< b\n"), "only at the start of a line");
  assert.ok(!hasMarkers("========\n"), "eight is not a marker");
});

test("a choice makes the block's text — mine and theirs through the rebase swap, both in either order, an edit as typed — and the file composes", () => {
  const c = parseConflicts(MERGE).segments[1];
  assert.equal(textFor(c, { choice: "mine" }, false), "mine\n");
  assert.equal(textFor(c, { choice: "theirs" }, false), "theirs\n");
  assert.equal(textFor(c, { choice: "mine" }, true), "theirs\n", "under a rebase mine is git's theirs");
  assert.equal(textFor(c, { choice: "theirs" }, true), "mine\n");
  assert.equal(textFor(c, { choice: "both" }, false), "mine\ntheirs\n");
  assert.equal(textFor(c, { choice: "both_reversed" }, false), "theirs\nmine\n");
  assert.equal(textFor(c, { choice: "edit", text: "merged\n" }, false), "merged\n");
  assert.equal(textFor(c, null, false), c.raw, "no choice keeps the markers");
  assert.equal(textFor({ ours: "x", theirs: "y\n", raw: "" }, { choice: "both" }, false), "x\ny\n", "a newline between when the first run lacks one");

  const segments = parseConflicts(MERGE).segments;
  let choices = choose({}, "c1", "theirs");
  choices = choose(choices, "c2", "edit", "hand\n");
  assert.equal(compose(segments, choices, false), "a\ntheirs\nb\nc\nhand\n");
  assert.equal(compose(segments, choose({}, "c1", "mine"), false), "a\nmine\nb\nc\n<<<<<<< HEAD\nm2\n=======\nt2\n>>>>>>> feature\n", "an unsettled conflict keeps its markers in the result");
  assert.ok(hasMarkers(compose(segments, choose({}, "c1", "mine"), false)));
  assert.deepEqual(choose({}, "c1", "nonsense"), {}, "an unknown choice is no choice");
  assert.deepEqual(unchoose(choices, "c2"), { c1: { choice: "theirs" } });
  assert.deepEqual(chooseAll(segments, "mine"), { c1: { choice: "mine" }, c2: { choice: "mine" } });
  assert.equal(compose(segments, chooseAll(segments, "theirs"), false), "a\ntheirs\nb\nc\nt2\n");
  assert.deepEqual([...CHOICES], ["mine", "theirs", "both", "both_reversed", "edit"]);
});

test("progress counts the settled conflicts, and the next and previous unsettled ones are found in order, wrapping", () => {
  const segments = parseConflicts(MERGE).segments;
  assert.deepEqual(progress(segments, {}), { settled: 0, total: 2 });
  assert.deepEqual(progress(segments, choose({}, "c1", "mine")), { settled: 1, total: 2 });
  assert.deepEqual(unsettledIds(segments, choose({}, "c1", "mine")), ["c2"]);
  assert.equal(nextUnsettled(segments, {}, null), "c1");
  assert.equal(nextUnsettled(segments, {}, "c1"), "c2");
  assert.equal(nextUnsettled(segments, {}, "c2"), "c1", "wraps");
  assert.equal(nextUnsettled(segments, choose({}, "c2", "mine"), "c1"), "c1", "the only unsettled one is itself");
  assert.equal(nextUnsettled(segments, chooseAll(segments, "mine"), "c1"), "c2", "all settled: any conflict, in order");
  assert.equal(previousUnsettled(segments, {}, "c1"), "c2");
  assert.equal(previousUnsettled(segments, {}, null), "c2", "from nothing, the previous is the last");
  assert.equal(nextUnsettled(segments, {}, "gone"), "c1", "an id the file no longer has counts as nothing");
  assert.equal(previousUnsettled(segments, {}, "gone"), "c2");
  assert.equal(nextUnsettled(segments, choose({}, "c1", "mine"), "c1"), "c2");
  assert.equal(previousUnsettled(segments, choose({}, "c2", "mine"), "c2"), "c1", "the settled one is skipped both ways");
  assert.equal(nextUnsettled([{ kind: "text", text: "x" }], {}, null), null);
  assert.equal(conflictsOf(null).length, 0);
});

test("the words: a fold counts lines, a settled card names its choice and side, the document its progress", () => {
  assert.equal(lineCount("a\nb\n"), 2);
  assert.equal(lineCount("a\nb"), 2);
  assert.equal(lineCount(""), 0);
  assert.equal(foldWords(42), "… 42 unchanged lines …");
  assert.equal(foldWords(1), "… 1 unchanged line …");
  const sides = { mine: { name: "main" }, theirs: { name: "feature/login" } };
  assert.equal(blockWords({ choice: "mine" }, sides), "kept mine — main");
  assert.equal(blockWords({ choice: "theirs" }, sides), "kept theirs — feature/login");
  assert.equal(blockWords({ choice: "both" }, sides), "kept both, mine first");
  assert.equal(blockWords({ choice: "both_reversed" }, sides), "kept both, theirs first");
  assert.equal(blockWords({ choice: "edit" }, sides), "edited");
  assert.equal(blockWords(null, sides), "not settled yet");
  assert.equal(documentWords({ settled: 0, total: 0 }), "No conflict markers are left in this file — review it and mark it resolved.");
  assert.equal(documentWords({ settled: 1, total: 3 }), "3 conflicts in this file · 1 settled · 2 to go.");
  assert.equal(documentWords({ settled: 1, total: 1 }), "Every conflict is settled — one of one. Review the result, then mark it resolved.");
  assert.equal(documentWords({ settled: 3, total: 3 }), "Every conflict is settled — 3 of 3. Review the result, then mark it resolved.");
});
