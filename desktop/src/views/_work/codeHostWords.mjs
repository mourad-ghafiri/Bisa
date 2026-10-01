/**
 * The words for a code host, by kind (ide/08): GitHub, GitLab and Bitbucket
 * each have a name, a noun for the thing a branch becomes on them — a *pull
 * request* or a *merge request* — a CLI or none, a public host, a Settings
 * panel and a token page. Every surface that says the host's name or the
 * noun reads it here, so a merge request is never called a pull request
 * and a fake code host in a test is *the code host*. The kinds and their
 * public hosts are held equal to `crates/bisa-codehost/src/lib.rs` by
 * the test beside this file.
 */

import { t } from "../../i18n/l10n.mjs";

/** The kinds, as the wire spells them (`code_host` on a project, a connection, a capabilities answer). */
export const KINDS = Object.freeze(["github", "gitlab", "bitbucket"]);

export const PUBLIC_HOSTS = Object.freeze({ github: "github.com", gitlab: "gitlab.com", bitbucket: "bitbucket.org" });

const LABELS = Object.freeze({ github: "GitHub", gitlab: "GitLab", bitbucket: "Bitbucket" });
const CLIS = Object.freeze({ github: { program: "gh", label: t("work-code-host-words-github-cli") }, gitlab: { program: "glab", label: t("work-code-host-words-gitlab-cli") } });

/**
 * The kind behind a `code_host` word — one of the three, or `null` for a
 * fake, an unknown host, or nothing.
 * @param {string | null | undefined} codeHost
 * @returns {"github" | "gitlab" | "bitbucket" | null}
 */
export function kindOf(codeHost) {
  return KINDS.includes(codeHost ?? "") ? /** @type {"github" | "gitlab" | "bitbucket"} */ (codeHost) : null;
}

/** The name a person reads — *the code host* when the kind is not one of the three. */
export function hostLabel(codeHost) {
  const kind = kindOf(codeHost);
  return kind ? LABELS[kind] : t("work-code-host-words-code-host");
}

/** What a branch becomes on the host: a *pull request*, or GitLab's *merge request*. */
export function prNoun(codeHost) {
  return kindOf(codeHost) === "gitlab" ? t("work-code-host-words-merge-request") : t("work-publish-outcome-banner-pull-request");
}

/** The noun with a capital, for a title. */
export function prNounCap(codeHost) {
  return kindOf(codeHost) === "gitlab" ? t("work-code-host-words-merge-request-2") : t("work-new-workstream-dialog-pull-request");
}

/** The plural. */
export function prNouns(codeHost) {
  return `${prNoun(codeHost)}s`;
}

/** The CLI that speaks for the kind, or `null` when it has none (Bitbucket). */
export function cliName(codeHost) {
  const kind = kindOf(codeHost);
  return kind && kind in CLIS ? CLIS[/** @type {"github" | "gitlab"} */ (kind)] : null;
}

/** The Settings panel that holds the kind's accounts and sign-in. */
export function settingsTabFor(codeHost) {
  return kindOf(codeHost);
}

/**
 * The review verdicts the host takes — its capabilities' word, every one of
 * the three when a capabilities answer says nothing (a fake, an older node).
 * @param {{review_events?: readonly string[]} | null | undefined} caps
 * @returns {readonly ("approve" | "request_changes" | "comment")[]}
 */
export function reviewEventsOf(caps) {
  const events = caps?.review_events;
  return Array.isArray(events) && events.length > 0 ? events : ["approve", "request_changes", "comment"];
}
