/**
 * The words of a folder repository's strip — the notes' and the drawings'
 * (ide/04, *The notes repository*; 19 — Drawings) — pure rules over the
 * status the node answers for `GET /notes/git` and `GET /drawings/git`, so
 * the strip itself only draws.
 *
 * The strip is one line at the foot of a list: what changed, what origin
 * lacks, when the last commit was. It never nags — a record that was never
 * committed is a fact on a line, not a banner — and the editor has no git
 * chrome at all. The one difference between the two folders is whether a
 * pull is offered: a drawing's record arrives by sync, so its repository has
 * none, and `menuVerbs` says so in words.
 */

import { ago } from "../../i18n/format.mjs";
import { t } from "../../i18n/l10n.mjs";

/** The remote every notes push goes to; the node's own name for it. */
export const ORIGIN = "origin";

/**
 * What the strip reads: `3 changes · ↑1 · committed 2h ago`. Each part is
 * left out when it says nothing — a clean tree with nothing to push and no
 * commit yet reads only *No commits yet*.
 * @param {import("./repoStripModel.d.mts").RepoStatus} status
 * @param {number} [now] unix seconds
 */
export function stripLine(status, now = Date.now() / 1000) {
  const parts = [];
  if (status.in_progress) parts.push(t("notes-notes-git-progress", { in_progress: status.in_progress }));
  parts.push(status.changed === 0 ? t("notes-notes-git-nothing-commit") : t("notes-notes-git-change-changes", { changed: status.changed }));
  if (status.ahead > 0) parts.push(`↑${status.ahead}`);
  if (status.behind > 0) parts.push(`↓${status.behind}`);
  if (status.last_commit) parts.push(t("notes-notes-git-committed-ago", { ago: ago(status.last_commit.at, now) }));
  else parts.push(t("notes-notes-git-no-commits-yet"));
  return parts.join(" · ");
}

/**
 * Why *Commit* is off, or null when it is on. The identity is the one reason
 * with a door; the others are facts about the moment.
 * @param {import("./repoStripModel.d.mts").RepoStatus} status
 * @param {string} message
 */
export function commitBlockedReason(status, message) {
  if (status.in_progress) return t("notes-notes-git-progress-finish-terminal-first", { in_progress: status.in_progress });
  if (status.identity.source === "none") return t("notes-notes-git-nobody-set-commit-notes");
  if (status.changed === 0) return t("notes-notes-git-nothing-has-changed-since-last-commit");
  if (message.trim() === "") return t("notes-notes-git-commit-needs-message");
  return null;
}

/** Whether the reason above is the one *Who commits…* fixes. */
export function nobodyToCommit(status) {
  return status.identity.source === "none";
}

/**
 * The message a commit gets when nobody wrote one: the folder's word and the
 * day, so a history of these reads as a diary rather than as *update* forty
 * times.
 * @param {number} [now] unix seconds
 * @param {string} [subject] the folder's word — `Notes` or `Drawings`
 */
export function defaultMessage(now = Date.now() / 1000, subject = "Notes") {
  // Spelled here rather than by the locale tables: a commit subject is the
  // same word on every machine that reads the history.
  const d = new Date(now * 1000);
  return `${subject}, ${d.getDate()} ${MONTHS[d.getMonth()]} ${d.getFullYear()}`; // content, never translated: a commit subject
}

const MONTHS = Object.freeze(["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]);

/**
 * Which of the menu's verbs are on, and the one sentence for each that is
 * off. Push needs a commit and an origin; fetch and pull need an origin;
 * pull also needs an upstream to pull from — and a folder that offers no
 * pull (the drawings') says why instead.
 * @param {import("./repoStripModel.d.mts").RepoStatus} status
 * @param {boolean} [pulls] whether the folder offers a pull at all
 * @returns {import("./repoStripModel.d.mts").MenuVerbs}
 */
export function menuVerbs(status, pulls = true) {
  const noOrigin = status.remote === null ? t("notes-notes-git-no-origin-yet-set-origin-door") : null;
  const busy = status.in_progress ? t("notes-notes-git-progress-2", { in_progress: status.in_progress }) : null;
  const push = busy ?? noOrigin ?? (status.last_commit === null ? t("notes-notes-git-nothing-has-been-committed-yet") : null);
  const fetch = busy ?? noOrigin;
  const pull = pulls ? busy ?? noOrigin ?? (status.upstream === null ? t("notes-notes-git-nothing-pull-from-until-first-push") : null) : t("shell-repo-strip-no-pull-records-arrive-by-sync");
  return {
    push: { on: push === null, reason: push },
    fetch: { on: fetch === null, reason: fetch },
    pull: { on: pull === null, reason: pull },
  };
}

/**
 * The toast after a push, from the status that came back.
 * @param {import("./repoStripModel.d.mts").RepoStatus} status
 */
export function pushOutcomeWords(status) {
  const where = status.upstream ?? `${ORIGIN}/${status.branch ?? "main"}`;
  return status.ahead === 0 ? t("notes-notes-git-pushed-has-everything", { where }) : t("notes-notes-git-pushed-still-lacks", { where, ahead: status.ahead });
}

/**
 * The toast after a pull: what moved, in the words the node's outcome uses.
 * `after` is the status the pull's answer carries (`PullOutcomeBody.status`).
 * @param {import("./repoStripModel.d.mts").RepoStatus} before
 * @param {import("./repoStripModel.d.mts").RepoStatus} after
 */
export function pullOutcomeWords(before, after) {
  if (before.behind === 0) return t("notes-notes-git-already-up-date");
  return after.behind === 0 ? t("notes-notes-git-pulled-commit-commits-from", { behind: before.behind, ORIGIN }) : t("notes-notes-git-pulled-still-take", { behind: after.behind });
}

/**
 * A remote URL or path the node will accept: something, trimmed. The node
 * is the one that knows whether git can reach it; this only refuses blank.
 * @param {string} value
 */
export function remoteRefusal(value) {
  return value.trim() === "" ? t("notes-notes-git-paste-url-path-repository-notes-push") : null;
}

/**
 * The who-commits pair, refused in words when either half is missing.
 * @param {string} name
 * @param {string} email
 */
export function identityRefusal(name, email) {
  if (name.trim() === "") return t("notes-notes-git-name-needed");
  if (email.trim() === "" || !email.includes("@")) return t("notes-notes-git-email-address-needed");
  return null;
}
