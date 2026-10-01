/**
 * The words of the code host panels. Run with
 * `node --test desktop/src/views/_settings/codeHostAccountsModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  accountRows,
  addedWords,
  cliLine,
  connectionLine,
  defaultLine,
  defaultWords,
  envOverrideLine,
  envVar,
  helpersLine,
  installLines,
  loginWords,
  resolvesLine,
  signingInWords,
  storeHint,
  tokenWords,
  checksStillListed,
  signInExitKey,
  signInPlan,
} from "./codeHostAccountsModel.mjs";

const RUST = readFileSync(new URL("../../../../crates/bisa-codehost/src/lib.rs", import.meta.url), "utf8");

const health = (over = {}) => ({
  kind: "github",
  host: "github.com",
  label: "GitHub",
  cli: { program: "gh", installed: true, path: "/opt/homebrew/bin/gh", version: "2.63.2", accounts: [{ host: "github.com", login: "octocat", active: true, protocol: "https" }], detail: null },
  env_override: false,
  store: "file",
  accounts: [],
  helpers: [],
  helper_username: null,
  cli_login: "octocat",
  default: null,
  resolves: { login: "octocat", source: "cli" },
  token_page: "https://github.com/settings/tokens",
  ...over,
});

test("the environment variable names are the crate's", () => {
  for (const kind of ["github", "gitlab", "bitbucket"]) {
    assert.ok(RUST.includes(`"${envVar(kind)}"`), `${envVar(kind)} is declared in the crate`);
  }
  assert.ok(RUST.includes('"repo", "workflow"') || readFileSync(new URL("../../../../crates/bisa-codehost/src/github.rs", import.meta.url), "utf8").includes('&["repo", "workflow"]'), "the GitHub scopes the token words name");
  assert.match(tokenWords("github").scopes, /repo and workflow/);
  assert.match(tokenWords("gitlab").scopes, /api scope/);
  assert.equal(tokenWords("bitbucket").needsLogin, true, "Bitbucket's token is checked beside its login");
  assert.equal(tokenWords("github").needsLogin, false);
});

test("the CLI row says installed and signed in, not signed in, not installed, or none", () => {
  assert.deepEqual(cliLine(health()), { tone: "ok", text: "GitHub CLI 2.63.2 · signed in to github.com as @octocat · git over https", action: null });
  const two = health({ cli: { ...health().cli, accounts: [...health().cli.accounts, { host: "github.com", login: "ada-acme", active: false, protocol: null }] } });
  assert.match(cliLine(two).text, /as @octocat and 1 more/);
  const unsigned = cliLine(health({ cli: { ...health().cli, accounts: [], detail: "You are not logged into any GitHub hosts" } }));
  assert.equal(unsigned.action, "signin");
  assert.match(unsigned.text, /not signed in to github\.com \(You are not logged into/);
  const absent = cliLine(health({ cli: { program: "gh", installed: false, path: null, version: null, accounts: [], detail: null } }));
  assert.equal(absent.action, "install");
  assert.match(absent.text, /GitHub CLI is not installed/);
  const none = cliLine(health({ kind: "bitbucket", cli: null }));
  assert.equal(none.action, null);
  assert.match(none.text, /Bitbucket has no CLI of its own/);
  assert.deepEqual(installLines({ brew: "brew install gh", apt: "sudo apt install gh", winget: "winget install --id GitHub.cli", url: "https://cli.github.com" }).map((l) => l.label), ["Homebrew", "apt", "winget"]);
  assert.deepEqual(installLines(null), []);
});

test("who answers, in one sentence, or that nobody does", () => {
  assert.deepEqual(resolvesLine(health()), { tone: "ok", text: "Requests to GitHub go as @octocat — the GitHub CLI signed in on this machine." });
  assert.match(resolvesLine(health({ resolves: { login: "ada", source: "file" } })).text, /@ada — a stored token/);
  assert.match(resolvesLine(health({ resolves: { login: null, source: "env" } })).text, /the environment's token — BISA_GITHUB_TOKEN/);
  assert.match(resolvesLine(health({ kind: "gitlab", resolves: { login: "ada", source: "git" } })).text, /Requests to GitLab go as @ada — git's credential helper/);
  const nobody = resolvesLine(health({ resolves: null }));
  assert.equal(nobody.tone, "warn");
  assert.match(nobody.text, /Nothing answers for GitHub yet/);
});

test("the stored accounts, the default, the helpers and the environment read as before, for any kind", () => {
  const h = health({ accounts: [{ login: "octocat", source: "file" }, { login: "ada", source: "keyring" }], default: "ada", helpers: ["osxkeychain"], helper_username: "octocat", env_override: true, kind: "gitlab", host: "gitlab.com" });
  const rows = accountRows(h);
  assert.deepEqual(rows.map((r) => [r.login, r.isDefault]), [["ada", true], ["octocat", false]]);
  assert.equal(rows[0].sourceWords, "in the OS keyring");
  assert.match(envOverrideLine(h), /^BISA_GITLAB_TOKEN is set .* request to GitLab/);
  assert.equal(envOverrideLine(health()), null);
  assert.equal(defaultLine(h), "Repositories under no profile and with no pin of their own use @ada.");
  assert.match(defaultLine(health({ cli_login: "octocat" })), /speaks as @octocat, the CLI's account/);
  assert.match(defaultLine(health({ cli_login: null })), /Sign in with the CLI, add a token/);
  assert.match(defaultLine(health({ accounts: [{ login: "a", source: "file" }, { login: "b", source: "file" }] })), /Several accounts/);
  assert.equal(helpersLine(h).tone, "ok");
  assert.match(helpersLine(h).text, /osxkeychain helper as octocat/);
  assert.equal(helpersLine(health()).tone, "quiet");
  assert.match(storeHint(h), /Checked with GitLab.*0600 file/);
  assert.match(storeHint(health({ store: "keyring" })), /OS keyring/);
});

test("a check reads as a line, and the sign-in plan as a button", () => {
  assert.equal(connectionLine(null).text, "Not checked yet.");
  assert.match(connectionLine({ state: "connected", login: "ada", scopes: ["repo"], missing: ["workflow"], recommended_missing: [], organizations: ["acme"] }, "GitHub").text, /Connected as @ada · repo · organizations: acme — missing workflow/);
  assert.match(connectionLine({ state: "refused", reason: "Bad credentials" }, "GitLab").text, /^GitLab refused the credential — Bad credentials/);
  assert.equal(connectionLine({ state: "no_token" }, "GitHub").tone, "quiet");
  assert.deepEqual(loginWords({ kind: "cli", program: "gh", host: "github.com", words: "w", token_page: "t" }), { button: "Authenticate", blurb: "w", opens: "terminal" });
  assert.equal(loginWords({ kind: "install", program: "glab", hints: { brew: "b", apt: "a", winget: "w", url: "u" }, words: "w", token_page: "t" }).opens, "install");
  assert.equal(loginWords({ kind: "token", token_page: "t", words: "w" }).opens, "token");
  assert.equal(loginWords(null).opens, null);
  assert.match(addedWords("ada", { state: "connected", login: "ada", scopes: [], missing: [], recommended_missing: [], organizations: [] }, "GitHub"), /^@ada added — Connected as @ada/);
  assert.equal(addedWords("ada", null), "@ada added.");
  assert.match(defaultWords(null), /the CLI's/);
  assert.match(signingInWords("github"), /gh auth login/);
  assert.match(signingInWords("bitbucket"), /Sign in on the host/);
});

test("the CLI sign-in opens a terminal of this machine, or says why it cannot", () => {
  assert.deepEqual(signInPlan(true, "github", "GitHub", "github.com"), {
    ok: true,
    terminal: { scope: "machine", id: "home", label: "Sign in to GitHub", login: { kind: "github", host: "github.com" } },
  });
  assert.equal(signInPlan(true, "gitlab", "GitLab", null).terminal.login.host, "", "a host not read yet is the kind's own");
  const held = signInPlan(false, "github", "GitHub", "github.com");
  assert.equal(held.ok, false);
  assert.match(held.why, /add a token instead/);
});

test("the panel reads again exactly when another sign-in terminal of its kind exits", () => {
  const live = { key: "t1", exitedAt: null, login: { kind: "github" } };
  const other = { key: "t2", exitedAt: 50, login: { kind: "gitlab" } };
  const plain = { key: "t3", exitedAt: 60, login: null };
  assert.equal(signInExitKey([live, other, plain], "github"), "", "nothing of this kind exited");
  const exited = { ...live, exitedAt: 70 };
  assert.equal(signInExitKey([exited, other, plain], "github"), "t1:70");
  assert.equal(signInExitKey([exited, { key: "t4", exitedAt: 90, login: { kind: "github" } }], "github"), "t1:70,t4:90");
  assert.equal(signInExitKey(undefined, "github"), "");
});

test("a login that left the list takes its Check line with it, and nothing moves while every login stays", () => {
  const checks = { ada: { state: "connected" }, bob: { state: "no_token" } };
  assert.equal(checksStillListed(checks, ["ada", "bob", "cy"]), checks, "the same object: no render for nothing");
  assert.deepEqual(checksStillListed(checks, ["ada"]), { ada: { state: "connected" } });
  assert.deepEqual(checksStillListed(checks, []), {});
  assert.deepEqual(checksStillListed({}, ["ada"]), {});
});
