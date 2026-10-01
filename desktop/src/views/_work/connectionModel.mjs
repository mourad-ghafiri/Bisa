/**
 * The Connection card's words (ide/04 §The repository under About › Settings), pure over the
 * node's `RepoConnection`: what the checkout will use when it talks to its
 * remote — the remote and its protocol, the profile, who commits and where
 * that comes from, the transport, the account — and the cautions, each an id
 * the engine computed with a sentence. The card draws; this file decides what
 * each row says and in what tone, and which tab a caution points at.
 */

import { cliName, hostLabel, kindOf, settingsTabFor } from "./codeHostWords.mjs";
import { protocolWords } from "./remoteModel.mjs";
import { t } from "../../i18n/l10n.mjs";

/** The caution ids, as `crates/bisa-engine/src/ide/connection.rs` spells them. */
export const CAUTION_IDS = Object.freeze([
  "identity_differs_from_profile",
  "key_not_loaded",
  "no_account",
  "account_outside_owner",
  "credential_username_differs",
  "key_shared_across_accounts",
]);

/** The account sources, as the engine spells them — the chain, in its order. */
export const ACCOUNT_SOURCES = Object.freeze(["local", "profile", "global", "env", "only_stored", "cli", "git_helper", "none"]);

/**
 * The remote row: `host · owner/name`, the protocol chip, and the alias when
 * the URL named one. `null` for a checkout with no remote.
 * @param {import("../../types").RepoConnection["remote"]} remote
 */
export function remoteRow(remote) {
  if (!remote) return null;
  const protocol = protocolWords(remote.protocol);
  return {
    summary: remote.summary,
    url: remote.url,
    protocol: protocol.label,
    transport: protocol.transport,
    alias: remote.alias ?? null,
    owner: remote.owner ?? null,
    note: remote.alias ? t("work-connection-written-which-ssh-resolves", { alias: remote.alias, host: remote.host ?? "?" }) : null,
  };
}

/**
 * The profile row.
 * @param {import("../../types").RepoConnection["profile"]} profile
 */
export function profileRow(profile) {
  if (!profile) return { tone: "quiet", text: t("work-connection-none-global-config-applies") };
  return { tone: "ok", text: `${profile.label} (${profile.slug})` };
}

/**
 * Who commits, and where that comes from — the profile named when it is one.
 * @param {import("../../types").IdentityFacts} identity
 * @param {import("../../types").RepoConnection["profile"]} profile
 */
export function identityRow(identity, profile) {
  const who = identity.name && identity.email ? `${identity.name} <${identity.email}>` : null;
  if (!who) return { tone: "warn", who: null, origin: t("work-git-config-card-nobody-set-commit-here") };
  if (identity.source === "local") return { tone: "ok", who, origin: t("work-git-config-card-set-repository") };
  if (identity.profile) {
    const label = profile && profile.slug === identity.profile ? profile.label : identity.profile;
    return { tone: "ok", who, origin: t("work-connection-from-profile", { label }) };
  }
  return { tone: "quiet", who, origin: t("work-connection-from-global-git-config") };
}

/**
 * The transport row: over SSH the key ssh would offer and whether ssh-agent
 * holds it; over HTTPS git's helpers and the username they hold.
 * @param {import("../../types").Transport} transport
 * @returns {{tone: "ok" | "warn" | "quiet", text: string, detail: string | null}}
 */
export function transportRow(transport) {
  switch (transport.kind) {
    case "ssh": {
      const key = transport.key_name ?? (transport.key ? String(transport.key) : null);
      if (transport.env_override) {
        return { tone: "warn", text: t("work-connection-ssh-git-ssh-command-set"), detail: transport.key ? String(transport.key) : null };
      }
      if (!transport.ssh_configured) {
        return { tone: "quiet", text: key ? t("work-connection-ssh", { key }) : "SSH", detail: t("work-connection-node-started-without-ssh-so-which") };
      }
      if (!key) return { tone: "quiet", text: t("work-connection-ssh-none-keys-would-offered-ssh"), detail: null };
      const loaded = transport.loaded === true ? "loaded in ssh-agent" : transport.loaded === false ? "not loaded in ssh-agent" : "ssh-agent did not answer";
      return {
        tone: transport.loaded === false ? "warn" : "ok",
        text: t("work-connection-ssh-key-only", { key, loaded, flag: (transport.identities_only) ? "yes" : "no" }),
        detail: transport.key ? String(transport.key) : null,
      };
    }
    case "https": {
      const helpers = transport.helpers ?? [];
      if (helpers.length === 0) return { tone: "warn", text: t("work-connection-https-git-names-no-credential-helper"), detail: null };
      const who = transport.helper_username ? ` as ${transport.helper_username}` : "";
      const cred = transport.credential_username ? ` ${t("work-connection-repository-asks-helper", { credential_username: transport.credential_username })}` : "";
      return { tone: "ok", text: t("work-connection-https-through-git-s-helper-helpers", { helpers: helpers.join(", "), helpers2: helpers.length, who, cred }), detail: null };
    }
    case "local":
      return { tone: "quiet", text: t("work-connection-repository-machine-no-credential"), detail: null };
    default:
      return { tone: "quiet", text: t("work-connection-no-remote"), detail: null };
  }
}

/**
 * The account row: the login the code host will be asked as, and why.
 * @param {import("../../types").AccountFacts} account
 * @param {string | null} codeHost
 * @param {import("../../types").RepoConnection["profile"]} profile
 */
export function accountRow(account, codeHost, profile) {
  const label = hostLabel(codeHost);
  const cli = cliName(codeHost);
  const source = {
    local: t("work-connection-pinned-repository"),
    profile: profile ? t("work-git-config-card-from-profile", { profile: profile.label }) : t("work-connection-from-profile-2"),
    global: t("work-connection-default-account", { label }),
    only_stored: t("work-connection-one-account-stored"),
    cli: t("work-connection-signed-machine", { cli: cli?.label ?? t("work-connection-cli") }),
    git_helper: t("work-connection-git-s-credential-helper-holds-host"),
    env: t("work-connection-answers-whatever-named", { codeHost: envVarOf(codeHost) }),
    none: codeHost ? t("work-connection-none-sign-under-settings-git-code", { label }) : t("work-connection-no-code-host-behind-remote"),
  }[account.source] ?? account.source;
  if (account.source === "env") return { tone: "quiet", login: null, source };
  if (!account.login) return { tone: codeHost ? "warn" : "quiet", login: null, source };
  return { tone: "ok", login: `@${account.login}`, source };
}

/** The environment variable that overrides a kind's tokens — `BISA_<KIND>_TOKEN`. */
function envVarOf(codeHost) {
  const kind = kindOf(codeHost);
  return kind ? `BISA_${kind.toUpperCase()}_TOKEN` : t("work-connection-environment-s-token");
}

/**
 * A caution's tone and the settings tab that helps, by id — the account
 * cautions point at the remote's own code host panel.
 * @param {string} id
 * @param {string | null | undefined} [codeHost] the remote's `code_host`
 * @returns {{tone: "warn" | "danger", tab: "git" | "git-ssh" | "github" | "gitlab" | "bitbucket" | null}}
 */
export function cautionMeta(id, codeHost = null) {
  const hostTab = settingsTabFor(codeHost) ?? "github";
  switch (id) {
    case "identity_differs_from_profile":
      return { tone: "warn", tab: "git" };
    case "key_not_loaded":
      return { tone: "warn", tab: "git-ssh" };
    case "key_shared_across_accounts":
      return { tone: "warn", tab: "git" };
    case "no_account":
      return { tone: "warn", tab: hostTab };
    case "account_outside_owner":
      return { tone: "danger", tab: hostTab };
    case "credential_username_differs":
      return { tone: "danger", tab: "git" };
    default:
      return { tone: "warn", tab: null };
  }
}

/** Whether the Repository segment and the header should wear a caution. */
export function hasCautions(connection) {
  return Boolean(connection && connection.cautions && connection.cautions.length > 0);
}

/** The first caution's sentence, for a tooltip. */
export function firstCaution(connection) {
  return connection?.cautions?.[0]?.sentence ?? null;
}

/**
 * What the probes said, one line each.
 * @param {import("../../types").ConnectionCheck} check
 * @returns {{tone: "ok" | "warn" | "danger" | "quiet", text: string}[]}
 */
export function checkLines(check) {
  const lines = [];
  if (check.code_host) {
    const c = check.code_host;
    switch (c.state) {
      case "connected": {
        const orgs = c.organizations?.length ? t("work-connection-organizations-list", { list: c.organizations.join(", ") }) : "";
        const missing = c.missing?.length ? t("work-connection-missing-list", { list: c.missing.join(", ") }) : "";
        lines.push({ tone: missing ? "warn" : "ok", text: t("work-connection-code-host-connected", { login: c.login, orgs, missing }) });
        break;
      }
      case "refused":
        lines.push({ tone: "danger", text: t("work-connection-code-host-refused", { reason: c.reason }) });
        break;
      case "unreachable":
        lines.push({ tone: "warn", text: t("work-connection-code-host-unreachable", { reason: c.reason }) });
        break;
      default:
        lines.push({ tone: "quiet", text: t("work-connection-code-host-no-token") });
    }
  }
  if (check.access) {
    const a = check.access;
    lines.push(
      a.found
        ? { tone: a.push ? "ok" : "warn", text: a.push ? t("work-connection-repository-found-account-can-push") : t("work-connection-repository-found-account-cannot-push") }
        : { tone: "danger", text: t("work-connection-repository-not-found-account-private-somebody") },
    );
  }
  if (check.ssh) {
    const g = check.ssh;
    switch (g.state) {
      case "authenticated":
        lines.push({ tone: "ok", text: g.login ? t("work-connection-ssh-authenticated-as", { login: g.login }) : t("work-connection-ssh-authenticated") });
        break;
      case "refused":
        lines.push({ tone: "danger", text: t("work-connection-ssh-refused", { reason: g.reason }) });
        break;
      case "host_key_unknown":
        lines.push({ tone: "warn", text: t("work-connection-ssh-host-key-not-known-hosts", { reason: g.reason }) });
        break;
      default:
        lines.push({ tone: "warn", text: t("work-connection-ssh-unreachable", { reason: g.reason }) });
    }
  }
  if (check.ls_remote) {
    const ls = check.ls_remote;
    lines.push(
      ls.ok
        ? { tone: "ok", text: t("work-connection-git-ls-remote-reachable-branch-branches", { heads: ls.heads }) }
        : { tone: "danger", text: t("work-connection-git-ls-remote", { detail: ls.detail ?? t("work-connection-refused") }) },
    );
  }
  return lines;
}
