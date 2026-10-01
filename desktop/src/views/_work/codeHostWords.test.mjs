/**
 * The code host words. Run with
 * `node --test desktop/src/views/_work/codeHostWords.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { KINDS, PUBLIC_HOSTS, cliName, hostLabel, kindOf, prNoun, prNounCap, prNouns, reviewEventsOf, settingsTabFor } from "./codeHostWords.mjs";

const RUST = readFileSync(new URL("../../../../crates/bisa-codehost/src/lib.rs", import.meta.url), "utf8");

test("the kinds and their public hosts are the crate's, word for word", () => {
  const start = RUST.indexOf("pub enum CodeHostKind {");
  const block = RUST.slice(start, RUST.indexOf("\n}", start));
  const variants = [...block.matchAll(/^\s{4}([A-Z][A-Za-z]+),/gm)].map((m) => m[1].toLowerCase());
  assert.deepEqual([...KINDS], variants);
  for (const kind of KINDS) {
    const host = PUBLIC_HOSTS[kind];
    assert.ok(RUST.includes(`"${host}"`), `${host} is the crate's public host for ${kind}`);
  }
});

test("the words: a name, a noun, a CLI, a panel — and the code host for anything else", () => {
  assert.equal(hostLabel("github"), "GitHub");
  assert.equal(hostLabel("gitlab"), "GitLab");
  assert.equal(hostLabel("bitbucket"), "Bitbucket");
  assert.equal(hostLabel("fake"), "the code host");
  assert.equal(hostLabel(null), "the code host");
  assert.equal(kindOf("fake"), null);
  assert.equal(prNoun("gitlab"), "merge request");
  assert.equal(prNoun("github"), "pull request");
  assert.equal(prNoun(null), "pull request");
  assert.equal(prNounCap("gitlab"), "Merge request");
  assert.equal(prNouns("gitlab"), "merge requests");
  assert.deepEqual(cliName("github"), { program: "gh", label: "GitHub CLI" });
  assert.deepEqual(cliName("gitlab"), { program: "glab", label: "GitLab CLI" });
  assert.equal(cliName("bitbucket"), null, "Bitbucket Cloud has no CLI of its own");
  assert.equal(settingsTabFor("bitbucket"), "bitbucket");
  assert.equal(settingsTabFor("fake"), null);
});

test("the review verdicts are the capabilities' word, all three when it says nothing", () => {
  assert.deepEqual(reviewEventsOf({ review_events: ["approve", "comment"] }), ["approve", "comment"]);
  assert.deepEqual(reviewEventsOf({ review_events: [] }), ["approve", "request_changes", "comment"]);
  assert.deepEqual(reviewEventsOf(null), ["approve", "request_changes", "comment"]);
});
