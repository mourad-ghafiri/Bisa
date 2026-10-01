import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  ACCOUNT_SOURCES,
  CAUTION_IDS,
  accountRow,
  cautionMeta,
  checkLines,
  firstCaution,
  hasCautions,
  identityRow,
  profileRow,
  remoteRow,
  transportRow,
} from "./connectionModel.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const RUST = readFileSync(join(HERE, "../../../../crates/bisa-engine/src/ide/connection.rs"), "utf8");

/** The variants of one Rust enum, snake_cased the way `serde(rename_all)` spells them. */
function variants(name) {
  const start = RUST.indexOf(`pub enum ${name} {`);
  const block = RUST.slice(start, RUST.indexOf("\n}", start));
  return [...block.matchAll(/^\s{4}([A-Z][A-Za-z]+),/gm)].map((m) => m[1].replace(/([a-z0-9])([A-Z])/g, "$1_$2").toLowerCase());
}

const profile = { slug: "acme", label: "Acme", name: "Ada", email: "ada@acme.example", ssh_key: "/k/acme", account: "ada-acme" };

test("the caution ids and the account sources are the engine's, and every caution has a tone and a door", () => {
  assert.deepEqual([...CAUTION_IDS].sort(), variants("CautionId").sort());
  assert.deepEqual([...ACCOUNT_SOURCES].sort(), variants("AccountSource").sort());
  for (const id of CAUTION_IDS) {
    const meta = cautionMeta(id);
    assert.ok(["warn", "danger"].includes(meta.tone), id);
    assert.ok(["git", "git-ssh", "github"].includes(meta.tab), `${id} points at a settings tab`);
  }
  assert.equal(cautionMeta("key_not_loaded").tab, "git-ssh");
  assert.equal(cautionMeta("no_account").tab, "github");
  assert.equal(cautionMeta("no_account", "gitlab").tab, "gitlab", "the account cautions point at the remote's own host panel");
  assert.equal(cautionMeta("account_outside_owner", "bitbucket").tab, "bitbucket");
  assert.equal(cautionMeta("no_account", "fake").tab, "github", "a host this build does not know by kind falls back to GitHub's panel");
  assert.equal(cautionMeta("account_outside_owner").tone, "danger");
  assert.equal(cautionMeta("nonesuch").tab, null);
});

test("the remote row carries the protocol chip and names an alias", () => {
  assert.equal(remoteRow(null), null);
  const plain = remoteRow({ url: "git@github.com:acme/web.git", protocol: "scp", host: "github.com", alias: null, owner: "acme", name: "web", summary: "github.com · acme/web" });
  assert.equal(plain.protocol, "SSH");
  assert.equal(plain.transport, "ssh");
  assert.equal(plain.note, null);
  const alias = remoteRow({ url: "git@github-acme:acme/web.git", protocol: "scp", host: "github.com", alias: "github-acme", owner: "acme", name: "web", summary: "github.com · acme/web" });
  assert.match(alias.note, /written as github-acme.*github\.com/);
  assert.equal(remoteRow({ url: "https://github.com/acme/web", protocol: "https", host: "github.com", alias: null, owner: "acme", name: "web", summary: "x" }).protocol, "HTTPS");
});

test("who commits says where it came from — the profile by its label", () => {
  assert.deepEqual(profileRow(null), { tone: "quiet", text: "none — the global config applies" });
  assert.deepEqual(profileRow(profile), { tone: "ok", text: "Acme (acme)" });
  const fromProfile = identityRow({ name: "Ada", email: "ada@acme.example", source: "global", profile: "acme", origin: "file:/x" }, profile);
  assert.equal(fromProfile.origin, "from the Acme profile");
  assert.equal(fromProfile.who, "Ada <ada@acme.example>");
  assert.equal(identityRow({ name: "Grace", email: "g@x.y", source: "global", profile: null, origin: null }, null).origin, "from your global git config");
  assert.equal(identityRow({ name: "Bob", email: "b@x.y", source: "local", profile: null, origin: null }, profile).origin, "set for this repository");
  const nobody = identityRow({ name: null, email: null, source: "none", profile: null, origin: null }, null);
  assert.equal(nobody.tone, "warn");
  assert.equal(nobody.who, null);
});

test("the transport row says which key or helper a push will use", () => {
  const ssh = (over) => ({ kind: "ssh", key: "/k/acme", key_name: "id_ed25519_acme", loaded: true, identities_only: true, agent_available: true, env_override: false, ssh_configured: true, ...over });
  assert.equal(transportRow(ssh({})).text, "SSH with id_ed25519_acme — loaded in ssh-agent, this key only");
  assert.equal(transportRow(ssh({})).tone, "ok");
  assert.equal(transportRow(ssh({ loaded: false })).tone, "warn");
  assert.match(transportRow(ssh({ loaded: null })).text, /did not answer/);
  assert.match(transportRow(ssh({ env_override: true })).text, /GIT_SSH_COMMAND/);
  assert.match(transportRow(ssh({ ssh_configured: false })).detail, /without SSH/);
  assert.match(transportRow(ssh({ key: null, key_name: null, identities_only: false })).text, /none of your keys/);
  assert.equal(transportRow({ kind: "https", helpers: ["osxkeychain"], helper_username: "ada", credential_username: null }).text, "HTTPS through git's osxkeychain helper as ada");
  assert.match(transportRow({ kind: "https", helpers: ["osxkeychain", "gh"], helper_username: null, credential_username: "ada-acme" }).text, /helpers \(this repository asks the helper for ada-acme\)/);
  assert.equal(transportRow({ kind: "https", helpers: [], helper_username: null, credential_username: null }).tone, "warn");
  assert.equal(transportRow({ kind: "local" }).text, "a repository on this machine — no credential");
  assert.equal(transportRow({ kind: "none" }).text, "no remote");
});

test("the account row names the login and why it is the one", () => {
  const acct = (over) => ({ login: "ada-acme", source: "profile", stored: ["ada-acme"], env_override: false, ...over });
  assert.deepEqual(accountRow(acct({}), "github", profile), { tone: "ok", login: "@ada-acme", source: "from the Acme profile" });
  assert.equal(accountRow(acct({ source: "local" }), "github", null).source, "pinned to this repository");
  assert.equal(accountRow(acct({ source: "global" }), "github", null).source, "the default GitHub account");
  assert.equal(accountRow(acct({ source: "global" }), "gitlab", null).source, "the default GitLab account");
  assert.equal(accountRow(acct({ source: "only_stored" }), "github", null).source, "the one account stored");
  assert.equal(accountRow(acct({ login: "octocat", source: "cli" }), "github", null).source, "signed in with the GitHub CLI on this machine");
  assert.equal(accountRow(acct({ login: "octocat", source: "cli" }), "gitlab", null).source, "signed in with the GitLab CLI on this machine");
  assert.equal(accountRow(acct({ login: "octocat", source: "git_helper" }), "bitbucket", null).source, "git's credential helper holds it for this host");
  const none = accountRow(acct({ login: null, source: "none" }), "github", null);
  assert.equal(none.tone, "warn");
  assert.match(none.source, /Settings › Git & code hosts › GitHub/);
  assert.match(accountRow(acct({ login: null, source: "none" }), "gitlab", null).source, /› GitLab$/);
  assert.equal(accountRow(acct({ login: null, source: "none" }), null, null).tone, "quiet", "no code host, no account needed");
  const env = accountRow(acct({ login: null, source: "env", env_override: true }), "github", null);
  assert.equal(env.login, null);
  assert.match(env.source, /^BISA_GITHUB_TOKEN answers/);
});

test("the probes read as one line each, and the cautions are what the header wears", () => {
  const lines = checkLines({
    code_host: { state: "connected", login: "ada-acme", scopes: ["repo"], missing: ["workflow"], recommended_missing: [], organizations: ["acme"] },
    access: { found: true, push: false },
    ssh: { state: "host_key_unknown", reason: "Host key verification failed." },
    ls_remote: { ok: true, heads: 3, detail: null },
  });
  assert.equal(lines.length, 4);
  assert.match(lines[0].text, /connected as @ada-acme · organizations acme — missing workflow/);
  assert.equal(lines[0].tone, "warn");
  assert.match(lines[1].text, /cannot push/);
  assert.match(lines[2].text, /known_hosts/);
  assert.equal(lines[3].text, "git ls-remote: reachable — 3 branches on the remote");
  const refused = checkLines({ code_host: { state: "refused", reason: "Bad credentials" }, access: { found: false, push: false }, ssh: { state: "refused", reason: "Permission denied (publickey)." }, ls_remote: { ok: false, heads: 0, detail: "not authenticated" } });
  assert.deepEqual(refused.map((l) => l.tone), ["danger", "danger", "danger", "danger"]);
  assert.equal(checkLines({ code_host: null, access: null, ssh: null, ls_remote: null }).length, 0);
  assert.equal(hasCautions(null), false);
  assert.equal(hasCautions({ cautions: [] }), false);
  assert.equal(hasCautions({ cautions: [{ id: "key_not_loaded", sentence: "ssh-agent does not hold the key" }] }), true);
  assert.equal(firstCaution({ cautions: [{ id: "key_not_loaded", sentence: "ssh-agent does not hold the key" }] }), "ssh-agent does not hold the key");
  assert.equal(firstCaution({ cautions: [] }), null);
});
