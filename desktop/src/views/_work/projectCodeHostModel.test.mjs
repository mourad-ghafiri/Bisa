/**
 * The New Project dialog's code host line. Run with
 * `node --test desktop/src/views/_work/projectCodeHostModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { accountOptions, inspectBody, inspectionLine, pinWrite, signInCaution } from "./projectCodeHostModel.mjs";

const remote = (over = {}) => ({ url: "git@github-acme:acme/web.git", protocol: "scp", host: "github.com", alias: "github-acme", owner: "acme", name: "web", summary: "github.com · acme/web", ...over });
const inspection = (over = {}) => ({
  remote: remote(),
  code_host: { kind: "github", host: "github.com", label: "GitHub" },
  profile: { slug: "acme", label: "Acme", name: "Ada", email: "ada@acme.example", ssh_key: null, account: "ada-acme" },
  accounts: [
    { login: "ada-acme", source: "profile", note: "from the Acme profile" },
    { login: "octocat", source: "cli", note: "signed in with the GitHub CLI" },
    { login: "ada", source: "stored", note: "a stored token" },
  ],
  suggested: "ada-acme",
  cautions: [],
  ...over,
});

test("the line names the host, the repository, the protocol, the alias and who it speaks as", () => {
  assert.equal(inspectionLine(null), null);
  assert.equal(inspectionLine({ remote: null, code_host: null, profile: null, accounts: [], suggested: null, cautions: [] }), null);
  const line = inspectionLine(inspection());
  assert.deepEqual(line, { tone: "ok", text: "GitHub · acme/web · SSH via github-acme · as @ada-acme", hostLabel: "GitHub" });
  assert.equal(inspectionLine(inspection(), "octocat").text, "GitHub · acme/web · SSH via github-acme · as @octocat", "the person's choice wins");
  const gitlab = inspectionLine(inspection({ remote: remote({ url: "https://gitlab.com/acme/platform/web.git", protocol: "https", alias: null, owner: "acme/platform" }), code_host: { kind: "gitlab", host: "gitlab.com", label: "GitLab" }, suggested: null, cautions: ["nobody is signed in to GitLab"] }));
  assert.deepEqual(gitlab, { tone: "warn", text: "GitLab · acme/platform/web · HTTPS", hostLabel: "GitLab" });
  const unknown = inspectionLine(inspection({ remote: remote({ host: "git.acme.internal", alias: null }), code_host: null, suggested: null }));
  assert.equal(unknown.tone, "quiet");
  assert.match(unknown.text, /^git\.acme\.internal · acme\/web · SSH — not a code host this build knows by name/);
  assert.equal(unknown.hostLabel, null);
});

test("the account options carry where each comes from, the suggested one marked; a pin is a lowercased codehost.account", () => {
  const options = accountOptions(inspection());
  assert.deepEqual(
    options.map((o) => [o.login, o.words, o.suggested]),
    [
      ["ada-acme", "@ada-acme — from the Acme profile", true],
      ["octocat", "@octocat — signed in with the GitHub CLI", false],
      ["ada", "@ada — a stored token", false],
    ],
  );
  assert.equal(accountOptions(inspection({ accounts: [{ login: "x", source: "default", note: "" }], code_host: { kind: "gitlab", host: "gitlab.com", label: "GitLab" } }))[0].words, "@x — the default GitLab account");
  assert.equal(accountOptions(inspection({ accounts: [{ login: "x", source: "git_helper", note: "" }] }))[0].words, "@x — git's credential helper");
  assert.deepEqual(accountOptions(null), []);
  assert.deepEqual(pinWrite(" Ada-Acme "), { key: "codehost.account", value: "ada-acme" });
  assert.equal(pinWrite(""), null);
  assert.equal(pinWrite(null), null);
});

test("the sign-in caution names the panel that fixes it", () => {
  assert.equal(signInCaution(inspection()), null);
  assert.deepEqual(signInCaution(inspection({ cautions: ["nobody is signed in to GitHub: …"] })), { text: "nobody is signed in to GitHub: …", tab: "github" });
  assert.deepEqual(signInCaution(inspection({ code_host: null, cautions: ["x is not a code host this build knows by name"] })), { text: "x is not a code host this build knows by name", tab: null });
});

test("the inspection is asked about one thing by name: a clone's URL or an import's folder, and nothing while there is nothing to read", () => {
  assert.deepEqual(inspectBody("clone", " https://example.com/acme/web.git ", "/Users/ada/web"), { url: "https://example.com/acme/web.git" }, "never the folder beside the URL");
  assert.deepEqual(inspectBody("import", "https://example.com/acme/web.git", " /Users/ada/web "), { path: "/Users/ada/web" }, "never the URL beside the folder");
  assert.equal(inspectBody("new", "https://example.com/acme/web.git", "/Users/ada/web"), null, "a new project has no remote yet");
  assert.equal(inspectBody("clone", "  ", "/Users/ada/web"), null);
  assert.equal(inspectBody("import", "https://example.com/acme/web.git", ""), null);
});
