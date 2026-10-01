import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import {
  canCommit,
  COMMITTER_REASONS,
  committerReasonSentence,
  IDENTITY_SOURCES,
  identityBlockedReason,
  identityMoved,
  identitySentence,
  identityState,
  pinnable,
  pinOffer,
  suggestionWords,
} from "./gitIdentityModel.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const VCS = join(HERE, "../../../../crates/bisa-vcs/src/git.rs");
const ENGINE_IDENTITY = join(HERE, "../../../../crates/bisa-engine/src/identity.rs");

const snake = (s) => s.replace(/([a-z])([A-Z])/g, "$1_$2").toLowerCase();

function variantsOf(src, name) {
  const body = src.match(new RegExp(`pub enum ${name} \\{([\\s\\S]*?)\\n\\}`));
  assert.ok(body, `${name} in the Rust source`);
  return [...body[1].matchAll(/^\s{4}([A-Z][A-Za-z]*)[ ,{(]/gm)].map((m) => m[1]);
}

test("the sources mirror the vcs crate's IdentitySource, in its order", () => {
  const variants = variantsOf(readFileSync(VCS, "utf8"), "IdentitySource").map((v) => v.toLowerCase());
  assert.deepEqual([...IDENTITY_SOURCES], variants);
});

test("the reasons mirror the engine's CommitterReason, in its order", () => {
  const variants = variantsOf(readFileSync(ENGINE_IDENTITY, "utf8"), "CommitterReason").map(snake);
  assert.deepEqual([...COMMITTER_REASONS], variants);
});

const local = { name: "Ada", email: "ada@example.invalid", source: "local", global: null };
const inherited = { name: "Grace", email: "grace@example.invalid", source: "global", global: { name: "Grace", email: "grace@example.invalid" } };
const missing = { name: null, email: null, source: "none", global: null };

test("three states, and an unknown view asks rather than assumes", () => {
  assert.equal(identityState(local), "local");
  assert.equal(identityState(inherited), "inherited");
  assert.equal(identityState(missing), "missing");
  assert.equal(identityState(undefined), "missing");
  assert.equal(identityState({ source: "nonsense" }), "missing");
});

test("a commit is blocked only by a known missing identity, and the reason names where the fix lives", () => {
  assert.equal(canCommit(local), true);
  assert.equal(canCommit(inherited), true);
  assert.equal(canCommit(missing), false);
  assert.equal(canCommit(null), true, "not loaded yet is not a block");
  assert.match(identityBlockedReason(missing), /About › Checkout/);
  assert.match(identityBlockedReason(missing, "in the dialog"), /in the dialog\.$/);
  assert.equal(identityBlockedReason(local), null);
});

test("pinning is offered for an inherited pair and never for a local one", () => {
  assert.equal(pinnable(inherited), true);
  assert.equal(pinnable(local), false);
  assert.equal(pinnable(missing), false);
  assert.equal(pinnable({ ...missing, global: { name: "G", email: "g@x.y" } }), true);
});

test("every reason has its sentence, and a created project says where it was born", () => {
  for (const reason of COMMITTER_REASONS) {
    assert.match(committerReasonSentence(reason, "web", { origin: "workspace" }), /web/);
  }
  assert.match(committerReasonSentence("created", "web", { origin: "step", goal: "g", run: "r", step: "s", workflow: "w" }), /by a workflow step/);
  assert.match(committerReasonSentence("created", "web", { origin: "goal", goal: "g" }), /for a goal/);
  assert.match(committerReasonSentence("created", "web", { origin: "goal", goal: "g", step: { run: "r", step: "build", workflow: "d" } }), /by a step of its goal's design/, "a design's step says so");
  assert.doesNotMatch(committerReasonSentence("created", "web", { origin: "workspace" }), /step|goal/);
  assert.match(committerReasonSentence("settlement_refused", "web", null), /kept uncommitted/);
  assert.match(committerReasonSentence("commit_refused", "web", null), /refused/);
  assert.match(committerReasonSentence("nonsense", "web", null), /just created/, "an unknown reason reads as the plainest one");
});

test("every state has its sentence, and the missing one asks", () => {
  assert.match(identitySentence(local), /this repository/);
  assert.match(identitySentence(inherited), /global/);
  assert.match(identitySentence({ ...inherited, profile: "acme" }), /acme profile/, "a profile's identity names the profile");
  assert.equal(identityState({ ...inherited, profile: "acme" }), "inherited", "and stays inherited — it resolves outside the repository");
  assert.match(identitySentence(missing), /Nobody/);
});

test("who commits is re-read on the answer, the question and the setup moving — and on nothing else", () => {
  for (const t of ["committer_set", "committer_needed", "git_setup_changed"]) assert.equal(identityMoved(t), true, t);
  for (const t of ["settings_changed", "file_changed", "workstream_changed", ""]) assert.equal(identityMoved(t), false, t);
});

test("a connected account's identity is offered only to a repository nobody commits in, with the login named and the pair to write", () => {
  const suggested = { name: "Ada Lovelace", email: "7+ada@users.noreply.github.com", login: "ada" };
  const words = suggestionWords({ source: "none", suggested });
  assert.equal(words.button, "Commit as Ada Lovelace");
  assert.match(words.sentence, /Your ada account would commit as Ada Lovelace <7\+ada@users\.noreply\.github\.com>/);
  assert.deepEqual(words.ident, { name: "Ada Lovelace", email: "7+ada@users.noreply.github.com" });
  assert.equal(suggestionWords({ source: "local", name: "B", email: "b@c.d", suggested }), null, "an identity in place is not second-guessed");
  assert.equal(suggestionWords({ source: "none", suggested: null }), null);
  assert.equal(suggestionWords({ source: "none", suggested: { name: "", email: "x@y.z", login: "ada" } }), null, "a half record offers nothing");
  assert.equal(suggestionWords(null), null);
});

test("a repository found without an identity at a restart asks without a creation story", () => {
  const words = committerReasonSentence("unresolved", "web", { origin: "goal", goal: "g" });
  assert.match(words, /web/);
  assert.match(words, /refuses every commit/);
  assert.doesNotMatch(words, /just created|for a goal|refused:/);
});

test("the pin offer names the verb and the global pair, and is nothing for a local or missing pair", () => {
  assert.deepEqual(pinOffer(inherited), { button: "Pin the global pair here", ident: { name: "Grace", email: "grace@example.invalid" } });
  assert.equal(pinOffer(local), null, "already its own");
  assert.equal(pinOffer(missing), null, "nothing to pin");
  assert.equal(pinOffer(null), null);
});
