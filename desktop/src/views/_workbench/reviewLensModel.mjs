/**
 * The review lens over an open file (ide/03, ide/09): which of the server's
 * hunks is current, what previous/next does, which verbs are enabled and
 * why, and the words the lens's bar and its per-hunk action zones wear. No
 * DOM, no Monaco: `DiffEditor`'s view zones draw what this decides.
 *
 * The server computes every hunk (0-based line spans; `disk` is where it
 * sits in the file on disk now) — this module only picks among them and
 * says what is possible. A hunk's Undo needs a clean buffer: the server
 * undoes against the disk, so a dirty editor's Undo is disabled with a hint
 * rather than silently undoing text the person has not saved.
 */

import { docModeKey } from "./fileDocModel.mjs";
import { t } from "../../i18n/l10n.mjs";

/** Where a document's review-lens choice is remembered — the checkout's session, by root and path. */
export function lensStorageKey(rootKey, path) {
  return `${rootKey}|review-lens|${path}`;
}

/** Where the lens's layout — inline, or side by side — is remembered, per document. */
export function lensLayoutKey(rootKey, path) {
  return `${rootKey}|review-lens-layout|${path}`;
}

/** The lens's two layouts; inline — removed and added lines in one column, the rest folded — is how a file reads as its changes. */
export const LENS_LAYOUTS = Object.freeze(["inline", "side-by-side"]);
export const DEFAULT_LENS_LAYOUT = "inline";

/** The other layout — what the bar's toggle switches to. @param {string} layout */
export function toggleLensLayout(layout) {
  return layout === "side-by-side" ? "inline" : "side-by-side";
}

/** The word for a layout, on the toggle. @param {string} layout */
export function lensLayoutWords(layout) {
  return layout === "side-by-side" ? t("workbench-review-lens-side-side") : t("workbench-review-lens-inline");
}

/**
 * What opening a file *for review* forces, whatever the document remembered:
 * Source mode — the lens draws only there — and the lens on. The caller
 * writes the mode under the document's key and the lens's draft, then opens
 * the file through the ordinary path door.
 * @param {string} rootKey
 * @param {string} path
 */
export function reviewOpenDrafts(rootKey, path) {
  return { modeKey: docModeKey(rootKey, path), mode: "source", lensKey: lensStorageKey(rootKey, path), lensOn: true };
}

/** *This file has 3 changes to review* — what a rendered document says while its diff waits in Source. @param {number} n */
export function pendingReviewWords(n) {
  return { text: t("workbench-review-lens-file-has-change-changes-review", { n }), show: t("workbench-review-lens-show-diff") };
}

/** Whether there is a hunk before/after the current index. */
export function hasPreviousHunk(hunks, index) {
  return (hunks ?? []).length > 0 && index > 0;
}
export function hasNextHunk(hunks, index) {
  return index < (hunks ?? []).length - 1;
}

/** The previous/next hunk's index, clamped — the bar's buttons disable rather than wrap. */
export function previousHunkIndex(hunks, index) {
  return hasPreviousHunk(hunks, index) ? index - 1 : index;
}
export function nextHunkIndex(hunks, index) {
  return hasNextHunk(hunks, index) ? index + 1 : index;
}

/** *Change 2 of 5*, or *No changes* for an opaque file or an empty list. */
export function hunkPositionWords(hunks, index) {
  const n = (hunks ?? []).length;
  if (n === 0) return t("workbench-review-lens-no-changes");
  const at = Math.min(Math.max(index, 0), n - 1);
  return t("workbench-review-lens-change", { at: at + 1, n });
}

/** Whether a hunk's Undo is enabled, and the hint when it is not — the buffer must be clean; the server undoes against the disk. */
export function undoEnabled(dirty) {
  return !dirty;
}
export const UNDO_DIRTY_HINT = t("workbench-review-lens-save-first-server-undoes-hunk-against");

/** The words a hunk's action zone wears. */
export function hunkActionWords() {
  return { keep: t("workbench-editor-doc-keep"), undo: t("workbench-editor-doc-undo") };
}

/**
 * The lens bar's file-grain words — an opaque file only ever offers the first
 * two, and no hunk navigation. `left` is how many other files still wait,
 * said on *Next file*.
 * @param {number} [left]
 */
export function fileActionWords(left = 0) {
  return {
    keepFile: t("workbench-review-lens-keep-file"),
    undoFile: t("workbench-review-lens-undo-file"),
    nextFile: left > 0 ? t("workbench-review-lens-next-file-left", { left }) : t("workbench-review-lens-next-file-review"),
    dropLens: t("workbench-review-lens-hide-review-lens"),
  };
}

/** Whether the lens has anything to draw for this file at all. */
export function lensApplies(file) {
  return !!file;
}

/**
 * What `settleChanges` sends for one hunk, with the file's `disk_hash` as it
 * was read (`FileReviewView.disk_hash`) — a 409 means the file moved since;
 * the caller refetches the file view and says so.
 */
export function hunkSettleTarget(path, hunk, diskHash) {
  return { grain: "hunk", path, hunk: hunk.id, disk_hash: diskHash };
}

/** What `settleChanges` sends for the whole file. */
export function fileSettleTarget(path) {
  return { grain: "file", path };
}
