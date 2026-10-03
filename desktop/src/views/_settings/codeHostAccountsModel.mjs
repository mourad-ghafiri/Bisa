/**
 * The words of Settings › Git & code hosts › GitHub · GitLab · Bitbucket
 * (ide/08 §Connection is a request), pure over the node's answers:
 * `GET /codehost/{kind}/health` — the CLI (installed, version, signed in as
 * whom), every stored login with where its token lives, whether the
 * environment overrides them all, git's helpers and the username they hold,
 * the default account, who would answer with nothing named — and
 * `GET /codehost/{kind}/login`, how to sign in from here. One panel draws the
 * three; this file decides what each says and in what tone.
 */

import { cliName, hostLabel } from "../_work/codeHostWords.mjs";
import { t } from "../../i18n/l10n.mjs";

/** The environment variable that overrides a kind's stored tokens — the crate's name, held equal by the test. */
export function envVar(kind) {
  return `BISA_${String(kind).toUpperCase()}_TOKEN`;
}

/**
 * The CLI row: what the machine has, and what to do about it.
 * @param {import("../../types").CodeHostHealth | null | undefined} health
 * @returns {{tone: "ok" | "warn" | "quiet", text: string, action: "signin" | "install" | null}}
 */
export function cliLine(health) {
  if (!health) return { tone: "quiet", text: "", action: null };
  const label = hostLabel(health.kind);
  const cli = health.cli;
  if (!cli) return { tone: "quiet", text: t("settings-code-host-accounts-has-cli-own-platform-speaks-api", { label }), action: null };
  const name = cliName(health.kind)?.label ?? t("settings-code-host-accounts-cli");
  if (!cli.installed) {
    return { tone: "warn", text: t("settings-code-host-accounts-installed-install-sign-once-git-cli", { name }), action: "install" };
  }
  const version = cli.version ? ` ${cli.version}` : "";
  const active = cli.accounts.find((a) => a.active) ?? cli.accounts[0] ?? null;
  if (!active) {
    const detail = cli.detail ? ` (${cli.detail})` : "";
    return { tone: "warn", text: t("settings-code-host-accounts-installed-but-signed", { name, version, host: health.host, detail }), action: "signin" };
  }
  const more = cli.accounts.length > 1 ? ` ${t("settings-code-host-accounts-more", { accounts: cli.accounts.length - 1 })}` : "";
  const protocol = active.protocol ? ` ${t("settings-code-host-accounts-git-over", { protocol: active.protocol })}` : "";
  return { tone: "ok", text: t("settings-code-host-accounts-signed", { name, version, host: active.host, login: active.login, more, protocol }), action: null };
}

/**
 * Where to get the CLI, one line per package manager.
 * @param {import("../../types").InstallHints | null | undefined} hints
 */
export function installLines(hints) {
  if (!hints) return [];
  return [
    { label: t("settings-code-host-accounts-homebrew"), command: hints.brew },
    { label: "apt", command: hints.apt },
    { label: "winget", command: hints.winget },
  ].filter((l) => l.command);
}

/**
 * Who would answer for the kind with nothing named — the health's `resolves`.
 * @param {import("../../types").CodeHostHealth | null | undefined} health
 * @returns {{tone: "ok" | "warn", text: string}}
 */
export function resolvesLine(health) {
  if (!health) return { tone: "warn", text: "" };
  const label = hostLabel(health.kind);
  const r = health.resolves;
  if (!r) {
    return { tone: "warn", text: t("settings-code-host-accounts-nothing-answers-yet-pull-requests-cannot", { label }) };
  }
  const who = r.login ? `@${r.login}` : t("settings-code-host-accounts-environment-s-token");
  const from = {
    env: t("settings-code-host-accounts-node-s-environment", { kind: envVar(health.kind) }),
    file: t("settings-code-host-accounts-stored-token"),
    keyring: t("settings-code-host-accounts-stored-token-os-keyring"),
    cli: t("settings-code-host-accounts-signed-machine", { kind: cliName(health.kind)?.label ?? t("settings-code-host-panel-cli-2") }),
    git: t("settings-code-host-accounts-git-s-credential-helper"),
  }[r.source] ?? r.source;
  return { tone: "ok", text: t("settings-code-host-accounts-requests-go", { label, who, from }) };
}

/**
 * One account's status line, from a check's answer. `null` before the check.
 * @param {import("../../types").CodeHostConnection | null | undefined} connection
 * @param {string} label the host's name
 * @returns {{ tone: "ok" | "warn" | "danger" | "quiet", text: string }}
 */
export function connectionLine(connection, label = t("settings-code-host-accounts-code-host")) {
  if (!connection) return { tone: "quiet", text: t("settings-code-host-accounts-checked-yet") };
  switch (connection.state) {
    case "connected": {
      const scopes = connection.scopes.length > 0 ? ` · ${connection.scopes.join(", ")}` : "";
      const missing = connection.missing.length > 0 ? ` ${t("settings-code-host-accounts-missing", { missing: connection.missing.join(", ") })}` : "";
      const better = connection.recommended_missing?.length > 0 ? ` ${t("settings-code-host-accounts-better", { recommended_missing: connection.recommended_missing.join(", ") })}` : "";
      const orgs = connection.organizations?.length > 0 ? ` · ${t("settings-code-host-accounts-organizations", { organizations: connection.organizations.join(", ") })}` : "";
      return { tone: missing ? "warn" : "ok", text: t("settings-code-host-accounts-connected", { login: connection.login, scopes, orgs, missing, better }) };
    }
    case "refused":
      return { tone: "danger", text: t("settings-code-host-accounts-refused-credential-sign-again-forget-account", { label, reason: connection.reason }) };
    case "unreachable":
      return { tone: "warn", text: t("settings-code-host-accounts-did-answer", { label, reason: connection.reason }) };
    case "no_token":
      return { tone: "quiet", text: t("settings-code-host-accounts-credential-held-login") };
    default:
      return { tone: "quiet", text: t("settings-code-host-accounts-checked-yet") };
  }
}

/**
 * The stored account rows: each login, where its token lives, whether it is
 * the default. Sorted by login.
 * @param {import("../../types").CodeHostHealth | null | undefined} health
 * @returns {{login: string, source: string, isDefault: boolean, sourceWords: string}[]}
 */
export function accountRows(health) {
  if (!health) return [];
  return [...health.accounts]
    .sort((a, b) => a.login.localeCompare(b.login))
    .map((a) => ({
      login: a.login,
      source: a.source,
      isDefault: health.default === a.login,
      sourceWords: a.source === "keyring" ? t("settings-code-host-accounts-os-keyring") : t("settings-code-host-accounts-0600-file-workspace-s-identity-folder"),
    }));
}

/**
 * The sentence about the environment override, when it applies.
 * @param {import("../../types").CodeHostHealth | null | undefined} health
 */
export function envOverrideLine(health) {
  if (!health?.env_override) return null;
  return t("settings-code-host-accounts-env-override", { env: envVar(health.kind), host: hostLabel(health.kind) });
}

/**
 * What git itself will do for pushes and pulls over HTTPS, from the helpers
 * its config names — and the username the helper holds for the host.
 * @param {import("../../types").CodeHostHealth | null | undefined} health
 * @returns {{ tone: "ok" | "quiet", text: string } | null}
 */
export function helpersLine(health) {
  if (!health) return null;
  const helpers = health.helpers ?? [];
  if (helpers.length === 0) {
    return { tone: "quiet", text: t("settings-code-host-accounts-git-names-credential-helper-here-push") };
  }
  const who = health.helper_username ? ` ${t("settings-code-host-accounts-as-user", { user: health.helper_username })}` : "";
  return { tone: "ok", text: t("settings-code-host-accounts-pushes-pulls-over-https-use-git", { helpers: helpers.join(", "), helpers2: helpers.length, who }) };
}

/**
 * What the default account means, in one sentence.
 * @param {import("../../types").CodeHostHealth | null | undefined} health
 */
export function defaultLine(health) {
  if (!health) return "";
  if (health.default) return t("settings-code-host-accounts-repositories-under-profile-pin-their-own", { default: health.default });
  if (health.accounts.length === 1) return t("settings-code-host-accounts-one-account-stored-so-every-repository", { login: health.accounts[0].login });
  if (health.accounts.length > 1) return t("settings-code-host-accounts-several-accounts-stored-none-default-repository");
  if (health.cli_login) return t("settings-code-host-accounts-nothing-stored-every-repository-under-profile", { cli_login: health.cli_login });
  return t("settings-code-host-accounts-account-yet-sign-cli-add-token");
}

/**
 * The token field's hint — where a stored token goes.
 * @param {import("../../types").CodeHostHealth | null | undefined} health
 */
export function storeHint(health) {
  const label = hostLabel(health?.kind);
  return health?.store === "keyring"
    ? t("settings-code-host-accounts-checked-then-stored-os-keyring-under", { label })
    : t("settings-code-host-accounts-checked-then-stored-0600-file-under", { label });
}

/**
 * The token form's words per kind: the field's placeholder, whether a login
 * field is asked for beside it, and what the token needs.
 * @param {string} kind
 */
export function tokenWords(kind) {
  switch (kind) {
    case "gitlab":
      return { placeholder: t("settings-code-host-accounts-glpat"), needsLogin: false, scopes: t("settings-code-host-accounts-personal-access-token-api-scope") };
    case "bitbucket":
      return { placeholder: t("settings-code-host-accounts-api-token-app-password"), needsLogin: true, scopes: t("settings-code-host-accounts-api-token-app-password-checked-beside") };
    default:
      return {
        placeholder: "ghp_… or github_pat_…",
        needsLogin: false,
        scopes: t("settings-code-host-accounts-classic-token-needs-repo-workflow-scopes"),
      };
  }
}

/**
 * Why *Add account* cannot be pressed yet, or `null` when it can (or while
 * an add is already on its way — the button says *Checking…* itself). The
 * reason is said on the disabled button, never left for the person to guess.
 * @param {{ ready: boolean, token: string, sent: string | null, needsLogin: boolean, login: string }} form
 * @returns {string | null}
 */
export function addBlockedWords({ ready, token, sent, needsLogin, login }) {
  if (!ready) return t("settings-code-host-panel-waiting-connection-check");
  if (!token.trim()) return t("settings-code-host-accounts-paste-token-first");
  if (needsLogin && !login.trim()) return t("settings-code-host-accounts-type-login-first");
  if (token === sent) return t("settings-code-host-accounts-token-already-added");
  return null;
}

/**
 * The sign-in button and its blurb, from the node's plan.
 * @param {import("../../types").LoginPlan | null | undefined} plan
 * @returns {{button: string, blurb: string, opens: "terminal" | "install" | "token" | null}}
 */
export function loginWords(plan) {
  if (!plan) return { button: t("settings-code-host-accounts-authenticate"), blurb: "", opens: null };
  switch (plan.kind) {
    case "cli":
      return { button: t("settings-code-host-accounts-authenticate"), blurb: plan.words, opens: "terminal" };
    case "install":
      return { button: t("settings-code-host-accounts-get-token-instead"), blurb: plan.words, opens: "install" };
    default:
      return { button: t("settings-code-host-accounts-get-token"), blurb: plan.words, opens: "token" };
  }
}

/** The toast after an account is added. */
export function addedWords(login, connection, label = t("settings-code-host-accounts-code-host")) {
  const line = connectionLine(connection, label);
  return connection?.state === "connected" ? t("settings-code-host-accounts-added-connection", { login, line: line.text }) : t("settings-code-host-accounts-added", { login });
}

/** The toast after the default changed. */
export function defaultWords(login) {
  return login ? t("settings-code-host-accounts-default-account", { login }) : t("settings-code-host-accounts-default-account-repository-under-profile-uses");
}

/** The toast after a sign-in terminal opened. */
export function signingInWords(kind) {
  const cli = cliName(kind);
  return cli ? t("settings-code-host-accounts-terminal-running-auth-login-follow-panel", { program: cli.program }) : t("settings-code-host-accounts-sign-host-then-check-again");
}

/**
 * What the CLI's browser sign-in does from here: opens a terminal of the
 * shell rooted at this machine, or says why it cannot — the web preview has
 * no terminal, and a token is the other way in.
 * @param {boolean} canOpenTerminal @param {string} kind @param {string} label the kind as people read it
 * @param {string | null | undefined} host
 * @returns {{ok: true, terminal: {scope: "machine", id: "home", label: string, login: {kind: string, host: string}}} | {ok: false, why: string}}
 */
export function signInPlan(canOpenTerminal, kind, label, host) {
  if (!canOpenTerminal) return { ok: false, why: t("settings-code-host-accounts-signing-cli-needs-desktop-app-add") };
  return { ok: true, terminal: { scope: "machine", id: "home", label: t("settings-code-host-accounts-sign", { label }), login: { kind, host: host ?? "" } } };
}

/**
 * One word per sign-in terminal of this kind that has exited — it changes
 * exactly when another one exits, which is when the CLI may hold a
 * credential it did not before and the panel reads again. Empty while none has.
 * @param {readonly {key: string, exitedAt: number | null, login?: {kind: string} | null}[]} sessions
 * @param {string} kind
 */
export function signInExitKey(sessions, kind) {
  return (sessions ?? [])
    .filter((s) => s.login?.kind === kind && s.exitedAt !== null && s.exitedAt !== undefined)
    .map((s) => `${s.key}:${s.exitedAt}`)
    .join(",");
}

/**
 * The *Check* results still worth showing: a login that left the list —
 * forgotten, signed out of the CLI, no longer the CLI's — takes its line
 * with it, so an old verdict is never read beside a new account.
 * @template T @param {Record<string, T>} checks @param {readonly string[]} logins the logins listed now
 * @returns {Record<string, T>} the same object when nothing left
 */
export function checksStillListed(checks, logins) {
  const listed = new Set(logins ?? []);
  const kept = Object.entries(checks ?? {}).filter(([login]) => listed.has(login));
  return kept.length === Object.keys(checks ?? {}).length ? checks : Object.fromEntries(kept);
}
