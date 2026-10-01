/**
 * The notes repository from a fresh workspace to a shared one, as the strip
 * and the menu read it at each step: nothing yet, a first note, a commit,
 * an origin, a push, someone else's commits, a pull, a merge under way.
 * Run with `node --test desktop/src/scenarios/notesGit.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { commitBlockedReason, defaultMessage, menuVerbs, nobodyToCommit, pullOutcomeWords, pushOutcomeWords, remoteRefusal, stripLine } from "../shell/repo/repoStripModel.mjs";

const NOW = 1_800_000_000;
const ADA = { name: "Ada", email: "ada@example.com", source: "local", global: null, profile: null };
const NOBODY = { name: null, email: null, source: "none", global: null, profile: null };
const status = (over = {}) => ({ changed: 0, branch: "main", upstream: null, ahead: 0, behind: 0, remote: null, identity: ADA, last_commit: null, in_progress: null, ...over });
const on = (verbs) => Object.fromEntries(Object.entries(verbs).map(([k, v]) => [k, v.on]));

test("from an empty repository to a pushed one, each step says exactly what is possible next", () => {
  // A fresh workspace: nothing to commit, nowhere to push.
  let s = status();
  assert.equal(stripLine(s, NOW), "Nothing to commit · no commits yet");
  assert.equal(commitBlockedReason(s, "x"), "Nothing has changed since the last commit.");
  assert.deepEqual(on(menuVerbs(s)), { push: false, fetch: false, pull: false });

  // The first note is written.
  s = status({ changed: 1 });
  assert.equal(stripLine(s, NOW), "1 change · no commits yet");
  assert.equal(commitBlockedReason(s, ""), "A commit needs a message.");
  assert.equal(commitBlockedReason(s, defaultMessage(NOW)), null, "the diary's own message will do");
  assert.match(defaultMessage(NOW), /^Notes, \d{1,2} [A-Z][a-z]{2} \d{4}$/);

  // Committed. Push still has nowhere to go.
  s = status({ last_commit: { short: "abc1234", subject: "Notes", at: NOW - 60 } });
  assert.equal(stripLine(s, NOW), "Nothing to commit · committed 1m ago");
  assert.match(menuVerbs(s).push.reason, /No origin yet/);

  // An origin is pasted in — a blank is refused in words, a path or URL is taken.
  assert.match(remoteRefusal("   "), /Paste the URL/);
  assert.equal(remoteRefusal("git@example.com:team/notes.git"), null);
  s = status({ remote: "git@example.com:team/notes.git", ahead: 1, last_commit: { short: "abc1234", subject: "Notes", at: NOW - 60 } });
  assert.equal(stripLine(s, NOW), "Nothing to commit · ↑1 · committed 1m ago");
  assert.deepEqual(on(menuVerbs(s)), { push: true, fetch: true, pull: false }, "a pull needs the first push to set the upstream");
  assert.match(menuVerbs(s).pull.reason, /first push/);

  // Pushed: the upstream exists and lacks nothing.
  s = status({ remote: "git@example.com:team/notes.git", upstream: "origin/main", last_commit: { short: "abc1234", subject: "Notes", at: NOW - 60 } });
  assert.equal(pushOutcomeWords(s), "Pushed — origin/main has everything.");
  assert.deepEqual(on(menuVerbs(s)), { push: true, fetch: true, pull: true });
});

test("someone else's commits arrive, a pull takes them, and a merge under way freezes every verb until it is finished in a terminal", () => {
  const shared = { remote: "git@example.com:team/notes.git", upstream: "origin/main", last_commit: { short: "abc1234", subject: "Notes", at: NOW - 3 * 3600 } };
  let before = status({ ...shared, behind: 3 });
  assert.equal(stripLine(before, NOW), "Nothing to commit · ↓3 · committed 3h ago");
  let after = status({ ...shared, last_commit: { short: "def5678", subject: "Theirs", at: NOW - 20 } });
  assert.equal(pullOutcomeWords(before, after), "Pulled 3 commits from origin.");
  assert.equal(stripLine(after, NOW), "Nothing to commit · committed just now");
  assert.equal(pullOutcomeWords(status({ ...shared, behind: 3 }), status({ ...shared, behind: 2 })), "Pulled — 2 still to take.");
  assert.equal(pullOutcomeWords(status(shared), status(shared)), "Already up to date.");

  const merging = status({ ...shared, changed: 2, in_progress: "merge" });
  assert.match(stripLine(merging, NOW), /^A merge is in progress · 2 changes · /);
  assert.match(commitBlockedReason(merging, "Resolve"), /merge is in progress — finish it in a terminal first/);
  assert.deepEqual(on(menuVerbs(merging)), { push: false, fetch: false, pull: false });
  for (const verb of Object.values(menuVerbs(merging))) assert.match(verb.reason, /merge is in progress/);
});

test("with nobody set to commit, the identity is the one reason with a door, and it is said before the message is even looked at", () => {
  const s = status({ changed: 4, identity: NOBODY });
  assert.equal(nobodyToCommit(s), true);
  assert.equal(commitBlockedReason(s, ""), "Nobody is set to commit notes.", "the identity outranks the empty message");
  assert.equal(stripLine(s, NOW), "4 changes · no commits yet", "the strip states facts and never nags");
  assert.equal(nobodyToCommit(status({ identity: ADA })), false);
});
