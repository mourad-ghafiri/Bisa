import { strict as assert } from "node:assert";
import { test } from "node:test";
import {
  commitBlockedReason,
  defaultMessage,
  identityRefusal,
  menuVerbs,
  nobodyToCommit,
  pullOutcomeWords,
  pushOutcomeWords,
  remoteRefusal,
  stripLine,
} from "./repoStripModel.mjs";

const NOW = 1_800_000_000;

function status(over = {}) {
  return {
    changed: 0,
    branch: "main",
    upstream: null,
    ahead: 0,
    behind: 0,
    remote: null,
    identity: { name: "Ada", email: "ada@example.com", source: "local", global: null, profile: null },
    last_commit: null,
    in_progress: null,
    ...over,
  };
}

test("the strip reads what changed, what origin lacks, and when the last commit was", () => {
  assert.equal(stripLine(status(), NOW), "Nothing to commit · no commits yet");
  assert.equal(stripLine(status({ changed: 1 }), NOW), "1 change · no commits yet");
  assert.equal(
    stripLine(status({ changed: 3, ahead: 1, last_commit: { short: "abc1234", subject: "Notes", at: NOW - 7200 } }), NOW),
    "3 changes · ↑1 · committed 2h ago",
  );
  assert.equal(stripLine(status({ behind: 2, last_commit: { short: "a", subject: "b", at: NOW - 10 } }), NOW), "Nothing to commit · ↓2 · committed just now");
  assert.equal(stripLine(status({ last_commit: { short: "a", subject: "b", at: NOW + 600 } }), NOW), "Nothing to commit · committed just now", "a commit stamped ahead of this clock is skew, not a forecast");
  assert.match(stripLine(status({ last_commit: { short: "a", subject: "b", at: NOW - 40 * 86_400 } }), NOW), /· committed on [A-Z][a-z]{2} \d{1,2}$/, "past a week the strip names the day");
  assert.match(stripLine(status({ in_progress: "merge" }), NOW), /^A merge is in progress · /);
});

test("commit is off for one reason at a time, the identity first because it has a door", () => {
  assert.equal(commitBlockedReason(status({ changed: 2 }), "Add a note"), null);
  assert.equal(commitBlockedReason(status({ changed: 2, identity: { name: null, email: null, source: "none", global: null, profile: null } }), "x"), "Nobody is set to commit notes.");
  assert.equal(commitBlockedReason(status(), "x"), "Nothing has changed since the last commit.");
  assert.equal(commitBlockedReason(status({ changed: 2 }), "   "), "A commit needs a message.");
  assert.match(commitBlockedReason(status({ changed: 2, in_progress: "rebase" }), "x"), /rebase is in progress/);
  assert.equal(nobodyToCommit(status({ identity: { name: null, email: null, source: "none", global: null, profile: null } })), true);
  assert.equal(nobodyToCommit(status()), false);
});

test("the default message is the day, so a history of them reads as a diary", () => {
  // Noon UTC: the same calendar day in every zone this can run in.
  assert.equal(defaultMessage(Date.UTC(2026, 8, 7, 12) / 1000), "Notes, 7 Sep 2026");
});

test("push needs a commit and an origin; fetch an origin; pull an upstream too", () => {
  let v = menuVerbs(status());
  assert.equal(v.push.on, false);
  assert.match(v.push.reason, /No origin yet/);
  assert.equal(v.fetch.on, false);
  assert.equal(v.pull.on, false);

  v = menuVerbs(status({ remote: "/tmp/origin.git" }));
  assert.equal(v.push.on, false);
  assert.match(v.push.reason, /Nothing has been committed/);
  assert.equal(v.fetch.on, true);
  assert.equal(v.pull.on, false);
  assert.match(v.pull.reason, /first push/);

  v = menuVerbs(status({ remote: "/tmp/origin.git", last_commit: { short: "a", subject: "b", at: 1 }, upstream: "origin/main" }));
  assert.deepEqual(v, { push: { on: true, reason: null }, fetch: { on: true, reason: null }, pull: { on: true, reason: null } });

  v = menuVerbs(status({ remote: "/tmp/origin.git", upstream: "origin/main", last_commit: { short: "a", subject: "b", at: 1 }, in_progress: "merge" }));
  assert.equal(v.push.on, false);
  assert.match(v.push.reason, /merge is in progress/);
});

test("the toasts after a push and a pull say what moved", () => {
  assert.equal(pushOutcomeWords(status({ upstream: "origin/main" })), "Pushed — origin/main has everything.");
  assert.equal(pushOutcomeWords(status({ branch: "notes", ahead: 2 })), "Pushed — origin/notes still lacks 2.");
  assert.equal(pullOutcomeWords(status(), status()), "Already up to date.");
  assert.equal(pullOutcomeWords(status({ behind: 1 }), status()), "Pulled 1 commit from origin.");
  assert.equal(pullOutcomeWords(status({ behind: 3 }), status({ behind: 1 })), "Pulled — 1 still to take.");
});

test("the two dialogs refuse blanks in words and accept the rest", () => {
  assert.match(remoteRefusal("  "), /Paste the URL/);
  assert.equal(remoteRefusal("/tmp/origin.git"), null);
  assert.equal(identityRefusal("", "a@b"), "A name is needed.");
  assert.equal(identityRefusal("Ada", "nope"), "An email address is needed.");
  assert.equal(identityRefusal("Ada", "ada@example.com"), null);
});

test("a folder that offers no pull says so, and its commit message names it", () => {
  const verbs = menuVerbs(status({ remote: "git@example.com:team/drawings.git", upstream: "origin/main", last_commit: { short: "a", subject: "b", at: NOW } }), false);
  assert.equal(verbs.push.on, true);
  assert.equal(verbs.fetch.on, true);
  assert.equal(verbs.pull.on, false);
  assert.match(verbs.pull.reason, /sync/);
  assert.match(defaultMessage(NOW, "Drawings"), /^Drawings, \d{1,2} [A-Z][a-z]{2} \d{4}$/);
});
