import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  MAX_LOGIN_LEN,
  aliasesText,
  emptySpec,
  foreignWords,
  matchesWords,
  parseAliases,
  profileRow,
  removedWords,
  savedWords,
  slugFor,
  specBody,
  specOf,
  validateSpec,
  versionNote,
} from "./gitProfilesModel.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const RUST = readFileSync(join(HERE, "../../../../crates/bisa-core/src/git_profile.rs"), "utf8");

const good = () => ({
  label: "Acme",
  host: "github.com",
  owner: "acme",
  aliases: ["github-acme"],
  name: "Ada Lovelace",
  email: "ada@acme.example",
  ssh_key: "/Users/ada/.ssh/id_ed25519_acme",
  account: "ada-acme",
});

const view = (over) => ({
  slug: "acme",
  file: "/ws/identity/git/profiles/acme.gitconfig",
  globs: ["https://github.com/acme/**", "ssh://git@github.com/acme/**", "git@github.com:acme/**"],
  ...good(),
  ...over,
});

test("the login length is the core's, and a good spec has no problems", () => {
  assert.equal(MAX_LOGIN_LEN, Number(RUST.match(/pub const MAX_LOGIN_LEN: usize = (\d+);/)[1]));
  assert.deepEqual(validateSpec(good()), {});
  assert.deepEqual(validateSpec({ ...good(), ssh_key: null, account: null, aliases: [] }), {}, "the optional fields may be empty");
  assert.deepEqual(emptySpec().host, "github.com");
});

test("every field has a refusal that names it, ahead of the round trip", () => {
  const bad = (over) => validateSpec({ ...good(), ...over });
  assert.match(bad({ label: "  " }).label, /needs a name/);
  assert.ok(bad({ host: "git hub.com" }).host);
  assert.ok(bad({ host: "-github.com" }).host);
  assert.equal(bad({ owner: "acme/platform" }).owner, undefined, "a GitLab group path is an owner");
  assert.equal(bad({ owner: "ada_acme" }).owner, undefined, "underscores and dots are GitLab's and Bitbucket's");
  assert.equal(bad({ account: "ada.lovelace" }).account, undefined);
  assert.ok(bad({ owner: "acme//web" }).owner, "an empty segment");
  assert.ok(bad({ owner: "a".repeat(256) }).owner);
  assert.ok(bad({ owner: "-acme" }).owner);
  assert.ok(bad({ owner: "acme web" }).owner);
  assert.ok(bad({ aliases: ["-x"] }).aliases);
  assert.ok(bad({ name: "--dash" }).name);
  assert.ok(bad({ name: "two\nlines" }).name);
  assert.ok(bad({ email: "no-at" }).email);
  assert.ok(bad({ email: "a b@c.d" }).email);
  assert.ok(bad({ ssh_key: "~/.ssh/id" }).ssh_key, "not absolute");
  assert.ok(bad({ ssh_key: "/tmp/a key" }).ssh_key, "a space would need quoting");
  assert.ok(bad({ ssh_key: "/tmp/../x" }).ssh_key);
  assert.ok(bad({ account: "bad login" }).account);
  assert.equal(Object.keys(bad({ owner: "", email: "x" })).length, 2, "one problem per field");
});

test("a label earns a slug the way the core does", () => {
  assert.equal(slugFor("Acme Corp (work)"), "acme-corp-work");
  assert.equal(slugFor("  ACME  "), "acme");
  assert.equal(slugFor("--- 42 ---"), "42");
  assert.equal(slugFor("!!!"), null);
  assert.equal(slugFor(""), null);
  assert.equal(slugFor("a".repeat(100)).length, 64);
});

test("the aliases field is one line, and the rows say where, who and what they match", () => {
  assert.equal(aliasesText(["github-acme", "gh-work"]), "github-acme, gh-work");
  assert.deepEqual(parseAliases("github-acme, gh-work  extra"), ["github-acme", "gh-work", "extra"]);
  assert.deepEqual(parseAliases(""), []);
  const row = profileRow(view({}));
  assert.equal(row.title, "Acme");
  assert.equal(row.where, "github.com/acme · aliases github-acme");
  assert.equal(row.who, "Ada Lovelace <ada@acme.example>");
  assert.equal(row.account, "@ada-acme");
  assert.equal(row.matches, "matches https://github.com/acme/… and 2 more spellings");
  assert.equal(matchesWords(["git@github.com:acme/**"]), "matches git@github.com:acme/…");
  assert.equal(matchesWords([]), "");
  assert.equal(profileRow(view({ aliases: [], account: null })).where, "github.com/acme");
  assert.equal(specOf(view({})).owner, "acme");
  assert.equal(specOf(view({ ssh_key: undefined })).ssh_key, null);
});

test("the version note, the foreign include and the toasts", () => {
  assert.equal(versionNote({ profiles: [], foreign_includes: [], git_version: "2.50.1", hasconfig_supported: true }), null);
  assert.match(versionNote({ profiles: [], foreign_includes: [], git_version: "2.35.0", hasconfig_supported: false }), /2\.35\.0.*2\.36/);
  assert.equal(foreignWords({ condition: "gitdir:~/work/", path: "/Users/ada/.gitconfig-work" }), 'includeIf "gitdir:~/work/" → /Users/ada/.gitconfig-work');
  assert.equal(savedWords(view({})), "Profile Acme saved — acme on github.com commits as Ada Lovelace.");
  assert.match(removedWords("acme"), /acme removed.*global git config/);
});

test("what a save sends is the spec by name: the aliases as the field reads them, a blank key or account sent as none, nothing the spec does not have", () => {
  assert.deepEqual(specBody(good(), "github-acme, gh-acme"), {
    label: "Acme",
    host: "github.com",
    owner: "acme",
    aliases: ["github-acme", "gh-acme"],
    name: "Ada Lovelace",
    email: "ada@acme.example",
    ssh_key: good().ssh_key || null,
    account: good().account || null,
  });
  // A draft seeded from the list's row, or edited into carrying more: the row's own facts stay here.
  const drafted = { ...view({}), ...good(), ssh_key: "", account: "", touched: true };
  assert.deepEqual(Object.keys(specBody(drafted, "")), ["label", "host", "owner", "aliases", "name", "email", "ssh_key", "account"]);
  assert.deepEqual(specBody(drafted, ""), { label: "Acme", host: "github.com", owner: "acme", aliases: [], name: "Ada Lovelace", email: "ada@acme.example", ssh_key: null, account: null });
});
