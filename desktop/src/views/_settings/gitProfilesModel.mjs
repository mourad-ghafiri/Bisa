/**
 * The words and the checks of Settings › Git & code hosts › Identity's
 * **profiles** (ide/04 §Profiles by organization), pure over the node's
 * `GET /git/profiles`. A profile is who you are for one owner on one code
 * host — the author, the SSH key, the account — kept by the platform as one
 * git config file the global config includes for that owner's remotes. The
 * form's refusals here mirror the core's (`crates/bisa-core/src/git_profile.rs`),
 * so a bad value is refused before the round trip; the node is still the
 * authority.
 */

import { t } from "../../i18n/l10n.mjs";

/**
 * One login grammar for the three code hosts — GitHub's dashes, GitLab's dots
 * and underscores, Bitbucket's underscores: letters, digits, `.`, `_` and `-`,
 * starting with a letter or digit, at most 255. An owner is one login or a
 * GitLab group path — logins joined by `/`.
 */
export const MAX_LOGIN_LEN = 255;
const LOGIN = /^[A-Za-z0-9][A-Za-z0-9._-]*$/;
const OWNER = /^[A-Za-z0-9][A-Za-z0-9._-]*(?:\/[A-Za-z0-9][A-Za-z0-9._-]*)*$/;
const HOST = /^[A-Za-z0-9](?:[A-Za-z0-9.-]*[A-Za-z0-9])?$/;
const ALIAS = /^[A-Za-z0-9_][A-Za-z0-9._-]*$/;
const KEY_PATH = /^\/[A-Za-z0-9._/@+-]*$/;

/** An empty spec, for the *New profile* form. */
export function emptySpec() {
  return { label: "", host: "github.com", owner: "", aliases: [], name: "", email: "", ssh_key: null, account: null };
}

/**
 * The form's seed for an existing profile.
 * @param {import("../../types").GitProfileView} view
 */
export function specOf(view) {
  return {
    label: view.label,
    host: view.host,
    owner: view.owner,
    aliases: [...(view.aliases ?? [])],
    name: view.name,
    email: view.email,
    ssh_key: view.ssh_key ?? null,
    account: view.account ?? null,
  };
}

/**
 * What `PUT /git/profiles/{slug}` takes (`ProfileSpec`): the form's eight
 * fields **by name**, the aliases as the field reads them, a key or an
 * account left blank sent as none. The form's draft is edited through
 * spreads and the node refuses a key the spec does not have, so what leaves
 * is built here.
 * @param {import("../../types").ProfileSpec} spec the form's draft
 * @param {string} aliases the aliases field, as typed
 * @returns {import("../../types").ProfileSpec}
 */
export function specBody(spec, aliases) {
  return {
    label: spec.label,
    host: spec.host,
    owner: spec.owner,
    aliases: parseAliases(aliases),
    name: spec.name,
    email: spec.email,
    ssh_key: spec.ssh_key || null,
    account: spec.account || null,
  };
}

/**
 * Every problem in a spec, by field — the core's rules, ahead of the round
 * trip. Empty when the spec is good.
 * @param {import("../../types").ProfileSpec} spec
 * @returns {Record<string, string>}
 */
export function validateSpec(spec) {
  const problems = {};
  if (!String(spec.label ?? "").trim()) problems.label = t("settings-git-profiles-profile-needs-name");
  const host = String(spec.host ?? "").trim();
  if (!HOST.test(host) || host.length > 253) problems.host = t("settings-git-profiles-host-name-letters-digits-dots-dashes");
  const owner = String(spec.owner ?? "").trim().replace(/^\/+|\/+$/g, "");
  if (!OWNER.test(owner) || owner.length > MAX_LOGIN_LEN) problems.owner = t("settings-git-profiles-owner-user-organization-workspace-group-path");
  for (const alias of spec.aliases ?? []) {
    if (!ALIAS.test(String(alias).trim())) {
      problems.aliases = t("settings-git-profiles-ssh-host-alias-letters-digits-dots", { alias });
      break;
    }
  }
  const name = String(spec.name ?? "").trim();
  if (!name || name.startsWith("-") || /[\r\n]/.test(name)) problems.name = t("settings-git-profiles-name-git-will-take-one-line");
  const email = String(spec.email ?? "").trim();
  if (!email || email.startsWith("-") || /\s/.test(email) || email.split("@").length !== 2 || email.startsWith("@") || email.endsWith("@")) {
    problems.email = t("settings-git-profiles-email-has-one-something-both-sides");
  }
  const key = String(spec.ssh_key ?? "").trim();
  if (key && (!KEY_PATH.test(key) || key.includes(".."))) problems.ssh_key = t("settings-git-profiles-absolute-path-only-letters-digits-spaces");
  const account = String(spec.account ?? "").trim();
  if (account && (!LOGIN.test(account) || account.length > MAX_LOGIN_LEN)) problems.account = t("settings-git-profiles-code-host-login-letters-digits-dots");
  return problems;
}

/**
 * The slug a label earns — the core's rule: lowercased, runs of anything else
 * one dash, trimmed to the slug alphabet, at most 64. `null` when nothing survives.
 * @param {string} label
 */
export function slugFor(label) {
  let out = "";
  let dash = false;
  for (const c0 of String(label ?? "").trim()) {
    const c = c0.toLowerCase();
    if (/[a-z0-9]/.test(c)) {
      out += c;
      dash = false;
    } else if (!dash && out) {
      out += "-";
      dash = true;
    }
  }
  out = out.replace(/-+$/, "").slice(0, 64).replace(/-+$/, "");
  return /^[a-z0-9][a-z0-9_-]*$/.test(out) ? out : null;
}

/**
 * The aliases field as one line, and back.
 * @param {readonly string[]} aliases
 */
export function aliasesText(aliases) {
  return (aliases ?? []).join(", ");
}
export function parseAliases(text) {
  return String(text ?? "")
    .split(/[,\s]+/)
    .map((a) => a.trim())
    .filter(Boolean);
}

/**
 * One profile as the list draws it.
 * @param {import("../../types").GitProfileView} view
 */
export function profileRow(view) {
  const aliases = view.aliases?.length ? ` · ${t("settings-git-profiles-aliases", { aliases: view.aliases.join(", ") })}` : "";
  return {
    slug: view.slug,
    title: view.label,
    where: `${view.host}/${view.owner}${aliases}`,
    who: `${view.name} <${view.email}>`,
    key: view.ssh_key ?? null,
    account: view.account ? `@${view.account}` : null,
    matches: matchesWords(view.globs ?? []),
  };
}

/**
 * *matches https://github.com/acme/… and 2 more spellings* — the globs, for a
 * person: the first as an example, the rest counted.
 * @param {readonly string[]} globs
 */
export function matchesWords(globs) {
  if (!globs || globs.length === 0) return "";
  const first = globs[0].replace(/\*\*$/, "…");
  const rest = globs.length - 1;
  return rest > 0 ? t("settings-git-profiles-matches-more-spelling-spellings", { first, rest }) : t("settings-git-profiles-matches", { first });
}

/**
 * The sentence about this git, when it cannot evaluate the includes.
 * @param {import("../../types").GitProfilesView | null | undefined} view
 */
export function versionNote(view) {
  if (!view || view.hasconfig_supported) return null;
  return t("settings-git-profiles-git-cannot-evaluate-includeif-hasconfig-so", { git_version: view.git_version });
}

/**
 * A foreign include, for the list: the condition and the file, as the person
 * wrote them into their global config.
 * @param {import("../../types").ForeignInclude} include
 */
export function foreignWords(include) {
  return `includeIf "${include.condition}" → ${include.path}`; // for the machine
}

/** The toasts. */
export function savedWords(view) {
  return t("settings-git-profiles-profile-saved-commits", { view: view.label, owner: view.owner, host: view.host, view2: view.name });
}
export function removedWords(slug) {
  return t("settings-git-profiles-profile-removed-repositories-fall-back-global", { slug });
}
