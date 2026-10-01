/**
 * The words and rules of the dialogs that merge, rebase, cherry-pick and
 * revert from the Branches view (ide/04 §The Branches view): the merge
 * modes with a sentence each, the default the project's setting picks, the
 * confirmations — the verb as the button, one or two sentences, the
 * recovery promise left to the dialog's `SafetyNote` — and the order a pick
 * is sent in. Pure, so `node --test` reads it.
 */

import { VERB } from "./gitWords.mjs";
import { t } from "../../i18n/l10n.mjs";

/** The merge modes, in the dialog's order. */
export const MERGE_MODES = Object.freeze(["ff", "no_ff", "ff_only", "squash"]);

export const MERGE_MODE_WORDS = Object.freeze({
  ff: Object.freeze({ label: t("work-merge-fast-forward-when-possible"), meaning: t("work-merge-moves-branch-when-nothing-local-stands") }),
  no_ff: Object.freeze({ label: t("work-merge-always-merge-commit"), meaning: t("work-merge-merge-commit-even-when-fast-forward") }),
  ff_only: Object.freeze({ label: t("work-merge-fast-forward-only"), meaning: t("work-merge-moves-branch-refuses-never-merge-commit") }),
  squash: Object.freeze({ label: t("work-merge-squash-into-one-staged-change"), meaning: t("work-merge-every-commit-s-change-staged-one") }),
});

/**
 * The mode the dialog starts on, from the project's `git.merge_strategy`:
 * *merge* is a merge commit, *squash* a squash, *rebase* a fast-forward when
 * it can — and anything else the fast-forward default.
 * @param {unknown} setting
 * @returns {"ff" | "no_ff" | "ff_only" | "squash"}
 */
export function mergeDefault(setting) {
  switch (setting) {
    case "merge":
      return "no_ff";
    case "squash":
      return "squash";
    default:
      return "ff";
  }
}

/** Whether a mode makes a commit whose message the person may write. */
export function mergeTakesMessage(mode) {
  return mode === "no_ff" || mode === "ff";
}

/** The message git would write itself, as the field's placeholder. */
export function mergeMessagePlaceholder(source, target) {
  return t("work-merge-merge-branch-into", { source, target });
}

const STOP = t("work-merge-conflict-stops-here-resolve-card-above");

/**
 * The confirmation before a merge, with the mode's meaning as its body.
 * @param {string} source
 * @param {string} target
 * @param {"ff" | "no_ff" | "ff_only" | "squash"} mode
 */
export function mergeConsent(source, target, mode) {
  const words = MERGE_MODE_WORDS[mode] ?? MERGE_MODE_WORDS.ff;
  return { title: t("work-merge-into", { merge: VERB.merge, source, target }), body: `${words.meaning} ${STOP}`, confirm: VERB.merge, danger: false, kind: "tree" };
}

/**
 * The confirmation before a rebase.
 * @param {string} current
 * @param {string} upstream
 * @param {{autostash?: boolean, onto?: string | null}} [opts]
 */
export function rebaseConsent(current, upstream, { autostash = false, onto = null } = {}) {
  const target = onto ?? upstream;
  const commits = onto ? t("work-merge-commits-since-replayed-top", { upstream, onto }) : t("work-merge-commits-replayed-top");
  const stash = autostash ? t("work-merge-autostash-note") : "";
  return { title: t("work-merge-onto", { rebase: VERB.rebase, current, target }), body: t("work-merge-conflict-stops-rebase-here-resolve-card", { commits, stash }), confirm: VERB.rebase, danger: false, kind: "tree" };
}

/**
 * The confirmation before a cherry-pick of `count` commits.
 * @param {number} count
 * @param {{recordOrigin?: boolean, noCommit?: boolean}} [opts]
 */
export function pickConsent(count, { recordOrigin = false, noCommit = false } = {}) {
  const what = count === 1 ? t("work-merge-commit-applied") : t("work-merge-commits-applied-oldest-first", { count });
  const how = noCommit ? ` ${t("work-merge-one-staged-change-commit")}` : ` ${t("work-merge-new-commits-current-branch")}`;
  const origin = recordOrigin ? t("work-merge-record-origin-note") : "";
  return { title: t("work-merge-cherry-pick-commit-commits", { count }), body: t("work-merge-conflict-stops-cherry-pick-here-resolve", { what, how, origin }), confirm: "Cherry-pick", danger: false, kind: "tree" };
}

/**
 * What the node foresees for a merge, before it runs (`GET
 * …/git/merge-preview`), as the dialog's line: nothing yet, clean, the
 * files that would conflict, or no preview on this git. For a rebase the
 * answer over the two tips is a likelihood, said so.
 * @param {{supported: boolean, clean: boolean, paths: string[]} | null | undefined} preview null while it is read
 * @param {{rebase?: boolean}} [opts]
 * @returns {{text: string, tone: "dim" | "ok" | "warn"}}
 */
export function previewWords(preview, { rebase = false } = {}) {
  if (!preview) return { text: t("work-merge-looking-ahead-conflicts"), tone: "dim" };
  if (!preview.supported) return { text: t("work-merge-no-look-ahead-git-conflict-if"), tone: "dim" };
  if (preview.clean) return { text: rebase ? t("work-merge-no-conflicts-expected-between-two-tips") : t("work-merge-no-conflicts-expected"), tone: "ok" };
  const n = preview.paths.length;
  const named = preview.paths.slice(0, 4).join(", ") + (n > 4 ? t("work-merge-and-more-files", { more: n - 4 }) : "");
  return { text: t("work-merge-file-files-would-conflict-between-two", { n, flag: (rebase) ? "yes" : "no", named }), tone: "warn" };
}

/**
 * The commits to send, oldest first — git applies a pick in the order given
 * — from a list drawn newest first and the ids the person ticked.
 * @param {readonly string[]} selected
 * @param {readonly {id: string}[]} listed newest first
 */
export function pickOrder(selected, listed) {
  const chosen = new Set(selected);
  return listed
    .filter((c) => chosen.has(c.id))
    .map((c) => c.id)
    .reverse();
}

