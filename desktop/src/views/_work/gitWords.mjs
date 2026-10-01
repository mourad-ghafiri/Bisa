/**
 * One word per act, everywhere Git is on screen (ide/04). A verb is spelled
 * here once and read by every button, menu item, confirmation and toast, so
 * a person who learned *Discard* under Changes meets *Discard* in the hunk
 * view and in its dialog — never *throw away*, never *abandon*. The places
 * a sentence points at are here too, in one spelling.
 *
 * Pure, so `node --test` reads it; the components import the words and add
 * nothing of their own.
 */

import { standingOf } from "./gitFiles.mjs";
import { t } from "../../i18n/l10n.mjs";

/** The verbs, each the one word for its act. */
export const VERB = Object.freeze({
  /** Put a change in the index. */
  stage: t("work-git-files-stage"),
  /** Take a change out of the index; the file is untouched. */
  unstage: t("work-git-files-unstage"),
  /** A working-tree change goes back — to the index, or to HEAD for an unmerged path. */
  discard: t("work-settings-toolbar-discard"),
  /** A file, a branch, a tag, a remote, a note — gone (a file git never saw goes through the IDE's disposal). */
  delete: t("work-git-words-delete"),
  /** A stash entry leaves the list — git's own word for it. */
  drop: t("work-rebase-editor-drop"),
  /** An operation left half-done is taken back to where it started. */
  abort: t("work-git-words-abort"),
  /** An operation left half-done goes on, its conflicted paths settled. */
  continue: t("work-commit-action-dialogs-continue"),
  /** The commit an operation stopped on is left out, and it goes on. */
  skip: t("work-git-words-skip"),
  /** A commit is undone with a new one. */
  revert: t("work-commit-actions-revert"),
  /** HEAD moves to a branch. */
  switch: t("work-commit-actions-switch-2"),
  /** HEAD moves to a commit, off any branch. */
  detach: t("work-library-refs-detach"),
  /** A Safety entry is put back. */
  restore: t("work-git-words-restore"),
  /** A stash entry is applied and kept. */
  apply: t("work-git-words-apply"),
  /** A stash entry is applied and dropped. */
  pop: t("work-git-words-pop"),
  merge: t("work-merge-control-merge"),
  rebase: t("work-git-words-rebase"),
  rename: t("work-workstream-panel-rename"),
  commit: t("work-git-words-commit"),
  /** The last commit is rewritten — its message, and what is staged folded in. */
  amend: t("work-git-words-amend"),
  push: t("work-git-words-push"),
  /** A branch overwrites its upstream — with a lease, never plainly. */
  forcePush: t("work-git-words-force-push"),
  pull: t("work-git-words-pull"),
  fetch: t("work-new-workstream-dialog-fetch"),
  stash: t("work-git-words-stash"),
});

/** The places a sentence points at, one spelling each. */
export const PLACE = Object.freeze({
  changes: t("work-git-words-git-changes"),
  branches: t("work-git-words-git-branches"),
  safety: t("work-git-words-git-branches-safety"),
  history: t("work-git-words-git-history"),
  stashes: t("work-git-words-git-stashes"),
  checkout: t("work-new-workstream-dialog-about-checkout"),
  settings: t("work-git-words-about-settings"),
  inbox: t("work-git-words-inbox"),
});

/** The one sentence about the recovery ref every consented act writes first. */
export const SAFETY_SENTENCE = t("work-git-words-before-anything-moves-what-here-saved");

/** The short line a control wears: the promise in four words. */
export const SAFETY_LINE = t("work-git-words-saved-safety-first");

/**
 * A confirmation button's word: the verb, with the count when more than one
 * thing goes — *Delete 3 files*, never *Go ahead*.
 * @param {keyof typeof VERB} verb
 * @param {{count?: number, noun?: string}} [of]
 */
export function confirmLabel(verb, { count = 1, noun = t("work-git-words-files") } = {}) {
  const word = VERB[verb] ?? String(verb);
  return count > 1 ? t("work-git-words-verb-count-noun", { word, count, noun }) : word;
}

/**
 * A bulk verb's word with what it reaches: *Stage 3* on a section's row,
 * *Stage all* on the toolbar for the whole tree.
 * @param {"stage" | "unstage"} verb
 * @param {number} [count] absent for the whole tree
 */
export function bulkLabel(verb, count) {
  const word = VERB[verb] ?? String(verb);
  return count === undefined ? t("work-git-words-all", { word }) : `${word} ${count}`;
}

/** git's porcelain letter → the word a row wears. */
const LETTER_WORD = Object.freeze({
  M: "modified",
  A: "added",
  D: "deleted",
  R: "renamed",
  C: "copied",
  T: t("work-git-words-type-changed"),
  U: "conflict",
  "?": "untracked",
});

/**
 * The word a changed file's row wears for one of its sides — the index
 * letter for the staged side, the worktree letter for the unstaged — so a
 * file that is both staged and edited since reads *added* on one chip and
 * *modified* on the other, which is the fact. An untracked or unmerged file
 * has its own word on either side. The two letters stay as the tooltip.
 * @param {{index?: string | null, worktree?: string | null, untracked?: boolean, conflicted?: boolean} | null | undefined} row
 * @param {"staged" | "unstaged"} side
 */
export function kindWord(row, side) {
  if (!row) return "changed";
  if (row.conflicted) return LETTER_WORD.U;
  if (row.untracked) return LETTER_WORD["?"];
  const letter = side === "staged" ? row.index : row.worktree;
  if (typeof letter !== "string" || letter === "" || letter === ".") return "changed";
  return LETTER_WORD[letter] ?? "changed";
}

/** The tone a standing's chip wears: the index in the accent, the working tree quiet, a conflict a warning. */
export const STANDING_TONE = Object.freeze({
  staged: "accent",
  unstaged: "quiet",
  untracked: "quiet",
  conflicted: "warn",
});

/**
 * The chips a file's row wears, one per side it has, in order: the staged
 * side, the unstaged, or the one an untracked or unmerged file has. Each
 * names the patch it opens (`side`), the word for what happened on it, its
 * tone and the sentence its tooltip reads.
 * @param {{index?: string | null, worktree?: string | null, staged?: boolean, unstaged?: boolean, untracked?: boolean, conflicted?: boolean} | null | undefined} row
 * @returns {{id: "staged" | "unstaged" | "untracked" | "conflicted", side: "staged" | "unstaged", word: string, tone: string, hint: string}[]}
 */
export function standingChips(row) {
  const s = standingOf(row);
  const chip = (id, side) => ({ id, side, word: kindWord(row, side), tone: STANDING_TONE[id], hint: standingHint(id, kindWord(row, side)) });
  if (s.conflicted) return [chip("conflicted", "unstaged")];
  if (s.untracked) return [chip("untracked", "unstaged")];
  const chips = [];
  if (s.staged) chips.push(chip("staged", "staged"));
  if (s.unstaged) chips.push(chip("unstaged", "unstaged"));
  return chips;
}

/** The sentence a standing chip's tooltip reads — what the word is of, and what its patch shows. */
export function standingHint(id, word) {
  switch (id) {
    case "staged":
      return t("work-git-words-staged-index-against-head-opens-patch", { word });
    case "unstaged":
      return t("work-git-words-unstaged-working-tree-against-index-opens", { word });
    case "untracked":
      return t("work-git-words-untracked-git-has-never-seen-patch");
    default:
      return t("work-git-words-conflict-unmerged-resolve-then-stage-resolved");
  }
}

/** What one of the workstream card's chips means, for the legend — keyed by the chip id `cardChips` emits. */
const CHIP_MEANING = Object.freeze({
  state: t("work-git-words-workstream-s-state"),
  pr: t("work-git-words-pull-request-code-host"),
  missing: t("work-git-words-checkout-not-disk"),
  in_progress: "an operation git left half-done",
  ahead_base: t("work-git-words-commits-beyond-base"),
  behind_base: t("work-git-words-commits-base-not-here"),
  upstream: t("work-git-words-ahead-behind-upstream"),
  no_upstream: t("work-git-words-branch-has-no-upstream-yet"),
  staged: t("work-git-words-files-staged"),
  modified: t("work-git-words-files-modified-not-staged"),
  untracked: t("work-git-words-files-git-has-never-seen"),
  conflicts: t("work-git-words-unmerged-paths"),
});

/** Every chip id the legend knows, for the test that holds it equal to `cardChips`. */
export const CHIP_IDS = Object.freeze(Object.keys(CHIP_MEANING));

/**
 * The Changes view's one line about size: *5 files · 3 staged · 2
 * unstaged · 1 untracked · 1 conflict*, counting a file once however many
 * lists it is in. `null` when nothing changed.
 * @param {{staged: readonly {path: string}[], unstaged: readonly {path: string}[], untracked: readonly {path: string}[], conflicted: readonly {path: string}[]}} groups
 */
export function fileSummary(groups) {
  const paths = new Set();
  for (const g of ["staged", "unstaged", "untracked", "conflicted"]) for (const r of groups?.[g] ?? []) paths.add(r.path);
  if (paths.size === 0) return null;
  const parts = [t("work-git-words-file-files-count", { n: paths.size })];
  const n = (g) => groups[g]?.length ?? 0;
  if (n("staged")) parts.push(t("work-git-words-staged", { staged: n("staged") }));
  if (n("unstaged")) parts.push(t("work-git-words-unstaged", { unstaged: n("unstaged") }));
  if (n("untracked")) parts.push(t("work-changes-toolbar-untracked", { untracked: n("untracked") }));
  if (n("conflicted")) parts.push(t("work-git-words-conflict-conflicts", { conflicted: n("conflicted") }));
  return parts.join(" · ");
}

/** Which side of a file a patch shows. */
export function sideWords(staged) {
  return staged ? t("work-git-words-index-against-head") : t("work-git-words-worktree-against-index");
}

