/**
 * Settings › SSH keys' words and states, tested where they live — over
 * fixtures only: a synthetic key, a scripted greeting, a made-up config.
 * Nothing here reads a `~/.ssh`, asks ssh-agent, or connects anywhere.
 */

import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  AGENT_START_COMMAND,
  PASSPHRASE_NOTE,
  agentWords,
  algorithmWords,
  busyKey,
  checkWords,
  copiedForHostWords,
  generatedWords,
  greetingWords,
  hostBlockText,
  hostChoices,
  hostKeyPage,
  hostPages,
  hostRows,
  keyCards,
  keyRows,
  loadedWords,
  nextStep,
  rememberCheck,
  resolvedWords,
  shortFingerprint,
  suggestKeyName,
  validateKeyName,
} from "./sshModel.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));

const key = (over) => ({
  name: "id_ed25519_acme",
  path: "/Users/ada/.ssh/id_ed25519_acme",
  algorithm: "ssh-ed25519",
  comment: "ada@acme",
  fingerprint: "SHA256:abcdefghijklmnopqrstuvwxyz0123456789ABCDEFG",
  public_line: "ssh-ed25519 AAAA ada@acme",
  loaded: true,
  ...over,
});
const overview = (over) => ({
  dir: "/Users/ada/.ssh",
  keys: [key({}), key({ name: "id_rsa", path: "/Users/ada/.ssh/id_rsa", loaded: false })],
  agent: { available: true, keys: [] },
  hosts: [],
  known_git_hosts: ["github.com", "gitlab.com"],
  ...over,
});

test("a fingerprint folds for a row and the algorithm reads as a word", () => {
  assert.equal(shortFingerprint("SHA256:abcdefghijklmnopqrstuvwxyz0123456789ABCDEFG"), "SHA256:abcdefgh…BCDEFG");
  assert.equal(shortFingerprint("SHA256:short"), "SHA256:short");
  assert.equal(algorithmWords("ssh-ed25519"), "ed25519");
  assert.equal(algorithmWords("ssh-rsa"), "rsa");
  assert.equal(algorithmWords("ecdsa-sha2-nistp256"), "ecdsa");
  assert.equal(algorithmWords("sk-ssh-ed25519@openssh.com"), "ed25519 (security key)");
});

test("the key rows carry the loaded state in words that depend on ssh-agent", () => {
  const rows = keyRows(overview({}));
  assert.equal(rows[0].loadedWords, "loaded in ssh-agent");
  assert.equal(rows[1].loadedWords, "not loaded");
  assert.equal(rows[0].publicLine, "ssh-ed25519 AAAA ada@acme");
  const noAgent = keyRows(overview({ agent: { available: false, keys: [], reason: "no agent" } }));
  assert.equal(noAgent[0].loadedWords, "ssh-agent unavailable");
  assert.deepEqual(keyRows(null), []);
});

test("ssh-agent's card says what it holds, and offers the one remedy only when it is down", () => {
  assert.deepEqual(agentWords({ available: true, keys: [] }), { tone: "quiet", text: "ssh-agent is running and holds no key.", remedy: null });
  const one = agentWords({ available: true, keys: [{ bits: 256, fingerprint: "x", comment: "", algorithm: "ED25519" }] });
  assert.equal(one.tone, "ok");
  assert.equal(one.text, "ssh-agent holds 1 key.");
  const down = agentWords({ available: false, keys: [], reason: "Could not open a connection" });
  assert.equal(down.tone, "warn");
  assert.match(down.text, /not available — Could not open/);
  assert.match(down.text, /start it in a terminal/);
  assert.equal(down.remedy, AGENT_START_COMMAND);
  assert.match(AGENT_START_COMMAND, /ssh-agent -s/);
});

test("the host rows read a block's options case-insensitively", () => {
  const rows = hostRows([{ patterns: ["github-acme"], options: [["hostname", "github.com"], ["User", "git"], ["identityfile", "~/.ssh/id_ed25519_acme"], ["IdentitiesOnly", "yes"]] }]);
  assert.deepEqual(rows[0], { patterns: "github-acme", hostname: "github.com", user: "git", identity: "~/.ssh/id_ed25519_acme", identitiesOnly: true });
  assert.equal(hostRows([{ patterns: ["*"], options: [] }])[0].hostname, null);
  assert.deepEqual(hostRows(null), []);
});

test("the hosts a check can name are the node's git hosts, then the config's, each once", () => {
  const o = overview({
    known_git_hosts: ["github.com", "gitlab.com", "bitbucket.org"],
    hosts: [
      { patterns: ["gh-work"], options: [["hostname", "GitHub.com"]] },
      { patterns: ["lab"], options: [["hostname", "git.lab.example"]] },
      { patterns: ["*"], options: [] },
    ],
  });
  assert.deepEqual(hostChoices(o), ["github.com", "gitlab.com", "bitbucket.org", "git.lab.example"]);
  assert.deepEqual(hostChoices(null), []);
  assert.deepEqual(hostChoices(overview({ known_git_hosts: [] })), []);
});

test("a known git host has a public SSH-keys page; a self-hosted one has none, and the platform registers nothing", () => {
  assert.equal(hostKeyPage("github.com"), "https://github.com/settings/ssh/new");
  assert.equal(hostKeyPage(" GitLab.com "), "https://gitlab.com/-/user_settings/ssh_keys");
  assert.match(hostKeyPage("bitbucket.org"), /^https:\/\/bitbucket\.org\//);
  assert.match(hostKeyPage("codeberg.org"), /^https:\/\/codeberg\.org\//);
  assert.equal(hostKeyPage("git.lab.example"), null);
  assert.equal(hostKeyPage(""), null);
  assert.deepEqual(
    hostPages().map((p) => p.host),
    ["github.com", "gitlab.com", "bitbucket.org", "codeberg.org"],
  );
  assert.ok(hostPages().every((p) => p.url.startsWith("https://")));
  assert.equal(copiedForHostWords("github.com"), "Public key copied — paste it on github.com, then check the key here.");
});

test("a greeting reads as ok, refused, host key unknown or unreachable — never accepting a host key for you", () => {
  assert.equal(greetingWords({ state: "authenticated", login: "ada-acme" }, "github.com").text, "github.com knows this key as ada-acme.");
  assert.equal(greetingWords({ state: "authenticated", login: null }, "git.example").tone, "ok");
  assert.equal(greetingWords({ state: "refused", reason: "Permission denied (publickey)." }, "github.com").tone, "danger");
  const unknown = greetingWords({ state: "host_key_unknown", reason: "Host key verification failed." }, "github.com");
  assert.equal(unknown.tone, "warn");
  assert.match(unknown.text, /ssh -T git@github\.com/);
  assert.match(unknown.text, /never accepts/);
  assert.match(greetingWords({ state: "unreachable", reason: "timeout" }, "x").text, /unreachable — timeout/);
});

test("a check is remembered per key, replaces the last one on that key, and reads with when it ran", () => {
  const ok = { state: "authenticated", login: "ada" };
  let checks = rememberCheck({}, "id_ed25519_acme", "github.com", ok, 1000);
  checks = rememberCheck(checks, "id_rsa", "gitlab.com", { state: "refused", reason: "denied" }, 1010);
  assert.deepEqual(Object.keys(checks), ["id_ed25519_acme", "id_rsa"]);
  checks = rememberCheck(checks, "id_ed25519_acme", "gitlab.com", ok, 1100);
  assert.equal(checks.id_ed25519_acme.host, "gitlab.com", "the newer check replaces the older on the same key");
  assert.equal(checks.id_rsa.host, "gitlab.com", "another key's check is untouched");
  assert.deepEqual(rememberCheck(null, "k", "h", ok, 1), { k: { host: "h", greeting: ok, at: 1 } });
  const words = checkWords(checks.id_ed25519_acme, 1100 + 125);
  assert.equal(words.tone, "ok");
  assert.equal(words.text, "gitlab.com knows this key as ada.");
  assert.equal(words.when, "checked 2 min ago");
});

test("the next step walks the path: put the key on the host, check it, bind it in a profile", () => {
  const fresh = nextStep({ check: null });
  assert.equal(fresh.kind, "register");
  assert.equal(fresh.action, "check");
  assert.match(fresh.text, /Copy the public key to your account on the host/);
  const known = nextStep({ check: { host: "github.com", greeting: { state: "authenticated", login: "ada" }, at: 1 } });
  assert.equal(known.kind, "ready");
  assert.equal(known.action, "profile");
  assert.match(known.text, /github\.com knows this key/);
  assert.match(known.text, /profile/);
  const refused = nextStep({ check: { host: "gitlab.com", greeting: { state: "refused", reason: "denied" }, at: 1 } });
  assert.equal(refused.kind, "register");
  assert.equal(refused.action, "host");
  assert.match(refused.text, /gitlab\.com does not know this key yet/);
  const hostKey = nextStep({ check: { host: "git.lab.example", greeting: { state: "host_key_unknown", reason: "x" }, at: 1 } });
  assert.equal(hostKey.kind, "host_key");
  assert.equal(hostKey.action, null);
  assert.match(hostKey.text, /ssh -T git@git\.lab\.example/);
  const offline = nextStep({ check: { host: "github.com", greeting: { state: "unreachable", reason: "timeout" }, at: 1 } });
  assert.equal(offline.kind, "check");
  assert.equal(offline.action, "check");
});

test("the cards are the rows with their pinned check and next step", () => {
  const checks = rememberCheck({}, "id_rsa", "github.com", { state: "authenticated", login: "ada" }, 5);
  const cards = keyCards(overview({}), checks);
  assert.equal(cards.length, 2);
  assert.equal(cards[0].check, null);
  assert.equal(cards[0].next.kind, "register");
  assert.equal(cards[1].check.host, "github.com");
  assert.equal(cards[1].next.kind, "ready");
  assert.equal(cards[1].loadedWords, "not loaded", "ssh-agent is beside the path, not on it: a checked key can be unloaded");
  assert.deepEqual(keyCards(null, null), []);
});

test("what ssh would offer a host reads by the keys' names, offline", () => {
  const keys = [{ name: "id_ed25519_acme", path: "/Users/ada/.ssh/id_ed25519_acme" }];
  const resolved = { hostname: "github.com", user: "git", port: 22, identity_files: ["/Users/ada/.ssh/id_ed25519_acme"], identities_only: true, identity_agent: null };
  assert.equal(resolvedWords("github.com", resolved, keys), "github.com: ssh would offer id_ed25519_acme (this key only) as git.");
  assert.equal(
    resolvedWords("gh-work", { ...resolved, identities_only: false, port: 443, identity_files: ["/Users/ada/.ssh/id_ed25519_acme", "/Users/ada/.ssh/other"] }, keys),
    "gh-work (github.com): ssh would offer id_ed25519_acme, /Users/ada/.ssh/other as git on port 443.",
  );
  assert.match(resolvedWords("github.com", { ...resolved, identity_files: [] }, keys), /no key file as git — ssh-agent decides/);
  assert.equal(busyKey("load", "id_rsa"), "load:id_rsa");
});

test("a key name is suggested from the owner and checked the way the node checks it", () => {
  assert.equal(suggestKeyName("Acme Corp"), "id_ed25519_acme-corp");
  assert.equal(suggestKeyName(""), "id_ed25519");
  assert.equal(validateKeyName("id_ed25519_acme", []), null);
  assert.match(validateKeyName("", []), /file name/);
  assert.match(validateKeyName("-rf", []), /dash/);
  assert.match(validateKeyName(".hidden", []), /dot/);
  assert.match(validateKeyName("a/b", []), /Letters/);
  assert.match(validateKeyName("key.pub", []), /\.pub/);
  assert.match(validateKeyName("id_ed25519_acme", ["id_ed25519_acme"]), /already exists/);
  assert.match(validateKeyName("a".repeat(101), []), /100/);
});

test("the Host block text is what the node writes, and the words say what to do next", () => {
  const src = readFileSync(join(HERE, "../../../../crates/bisa-ssh/src/config.rs"), "utf8");
  assert.ok(src.includes("Host {alias}\\n    HostName {hostname}\\n    User {user}\\n"), "the Rust text is the model's");
  assert.equal(
    hostBlockText("github-acme", "github.com", "git", "/Users/ada/.ssh/id_ed25519_acme"),
    "Host github-acme\n    HostName github.com\n    User git\n    IdentityFile /Users/ada/.ssh/id_ed25519_acme\n    IdentitiesOnly yes\n",
  );
  assert.doesNotMatch(hostBlockText("gh", "github.com", "git", null), /IdentityFile/);
  assert.match(PASSPHRASE_NOTE, /ssh-keygen -p/);
  assert.match(generatedWords(key({})), /id_ed25519_acme generated — SHA256:.*then check that the host knows it/);
  assert.equal(loadedWords("id_ed25519_acme"), "id_ed25519_acme loaded into ssh-agent.");
});
