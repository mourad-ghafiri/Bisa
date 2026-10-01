import assert from "node:assert/strict";
import { test } from "node:test";
import { INIT_LABEL, PLAIN_FOLDER, initConsequence, initDoneWords, initOffer } from "./initRepositoryModel.mjs";

test("the offer belongs on a project's own root that is on disk and not a repository — nowhere else", () => {
  assert.equal(initOffer({ kind: "primary", exists: true, git: false }).shown, true);
  assert.equal(initOffer({ kind: "primary", exists: true, git: true }).shown, false, "a repository already");
  assert.equal(initOffer({ kind: "primary", exists: false, git: false }).shown, false, "not on disk: nothing to initialise");
  assert.equal(initOffer({ kind: "copy", exists: true, git: false }).shown, false, "a copy has no project-level thing to turn on");
  assert.equal(initOffer({ kind: "worktree", exists: true, git: true }).shown, false);
  assert.equal(initOffer({ kind: undefined, exists: undefined, git: undefined }).shown, false, "unknown facts offer nothing");
});

test("an adopted folder is asked about first, naming whose folder is written; a managed one is not", () => {
  const theirs = initConsequence({ adopted: true, path: "/Users/ada/legacy" });
  assert.equal(theirs.needsConfirm, true);
  assert.match(theirs.body, /writes a \.git folder into \/Users\/ada\/legacy — a folder Bisa did not make/);
  assert.match(theirs.body, /copy workstreams keep working; the next one is a branch/);
  const ours = initConsequence({ adopted: false, path: null });
  assert.equal(ours.needsConfirm, false);
  assert.doesNotMatch(ours.body, /did not make/);
  assert.match(ours.body, /branches instead of copying/);
  assert.match(initConsequence({ adopted: true, path: null }).body, /writes a \.git folder here/, "no path known: still asked, still honest");
});

test("the done words carry the committer policy's sentence when it had one", () => {
  assert.equal(initDoneWords(null), "Repository initialised.");
  assert.equal(initDoneWords("  "), "Repository initialised.");
  assert.equal(initDoneWords("nobody is set to commit yet; the person is asked"), "Repository initialised — nobody is set to commit yet; the person is asked.");
});

test("the sentence and the label are single, finished sentences", () => {
  assert.match(PLAIN_FOLDER, /^A plain folder — no repository\./);
  assert.equal(INIT_LABEL, "Initialise a repository");
});
