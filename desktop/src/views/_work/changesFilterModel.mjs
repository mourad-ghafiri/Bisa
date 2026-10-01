/**
 * The Changes view's filter (ide/04), as a fact about rows: which changed
 * files a word keeps, and how many each word would keep. The words and
 * their labels are the remembered choice's (`rightPanelModel.CHANGE_FILTERS`);
 * this is the predicate the tree draws through.
 *
 * `unstaged` and `untracked` never overlap — git says both of a new file,
 * and the panel has a word for each. `tracked` is everything git knows,
 * a conflicted path included. `modified` is a content change, the letter
 * `M` on either side: an added, deleted, renamed or copied file, and a new
 * one, are something else. `conflicted` is the unmerged paths alone — what
 * an operation left to settle (ide/04 §Conflicts, continued).
 */

import { CHANGE_FILTERS, CHANGE_FILTER_LABEL } from "../_workbench/rightPanelModel.mjs";
import { t } from "../../i18n/l10n.mjs";

/**
 * Whether a row is kept under a word.
 * @param {import("../../types").GitFileRow} row
 * @param {string} filter one of `CHANGE_FILTERS`
 */
export function admits(row, filter) {
  if (!row || typeof row.path !== "string") return false;
  switch (filter) {
    case "conflicted":
      return row.conflicted === true;
    case "staged":
      return row.staged === true;
    case "unstaged":
      return row.unstaged === true && row.untracked !== true;
    case "tracked":
      return row.untracked !== true;
    case "untracked":
      return row.untracked === true;
    case "modified":
      return row.index === "M" || row.worktree === "M";
    default:
      return true;
  }
}

/**
 * The rows a word keeps, in the order they came. `all` is the same array,
 * so a caller that keys on identity sees no change.
 * @param {readonly import("../../types").GitFileRow[] | null | undefined} files
 * @param {string} filter
 */
export function filterGitFiles(files, filter) {
  const rows = Array.isArray(files) ? files : [];
  if (filter === "all" || !CHANGE_FILTERS.includes(filter)) return rows;
  return rows.filter((row) => admits(row, filter));
}

/**
 * How many rows each word would keep, all seven at once — what the menu
 * prints beside each word and greys a word at zero by.
 * @param {readonly import("../../types").GitFileRow[] | null | undefined} files
 * @returns {Record<string, number>}
 */
export function filterCounts(files) {
  const rows = Array.isArray(files) ? files : [];
  const out = {};
  for (const f of CHANGE_FILTERS) out[f] = 0;
  for (const row of rows) {
    for (const f of CHANGE_FILTERS) if (admits(row, f)) out[f] += 1;
  }
  return out;
}

/**
 * What the toolbar's trigger says for the word on: the word alone for
 * *All*, else the word and its count — *Staged · 3*.
 * @param {string} filter
 * @param {number} count
 */
export function filterWords(filter, count) {
  const label = CHANGE_FILTER_LABEL[filter] ?? CHANGE_FILTER_LABEL.all;
  return filter === "all" ? label : `${label} · ${count}`;
}

/** What an emptied tree says under a word: *Nothing staged*, *Nothing untracked*, … */
export function emptyWords(filter) {
  const label = (CHANGE_FILTER_LABEL[filter] ?? "").toLowerCase();
  return label && filter !== "all" ? t("work-changes-filter-nothing", { label }) : t("work-changes-filter-nothing-show");
}
