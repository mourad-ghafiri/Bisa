/**
 * What a commit document reads under its file list (ide/05 §Click to
 * inspect), as facts: the file list is a selector, and the view chosen for
 * every patch (`patchViewModel.mjs`) says what the choice means. On
 * **Hunks** the whole patch is read until a file is chosen, and the chosen
 * file alone after — a click on the chosen row lets go of it. **Side by
 * side** and **Inline** compare one file's two texts, so they read the
 * chosen file, else the first, and nothing when the commit touched none.
 * Plain `.mjs`, so `node --test` reads it.
 */

import { needsSides } from "./patchViewModel.mjs";
import { t } from "../../i18n/l10n.mjs";

/**
 * @typedef {{path: string, old_path?: string | null, kind: string}} CommitFile
 * @typedef {{kind: "all"} | {kind: "file", path: string, oldPath: string | null, change: string}} CommitFocus
 */

/** The focus for one file of the list. @param {CommitFile} file @returns {CommitFocus} */
function fileFocus(file) {
  return { kind: "file", path: file.path, oldPath: file.old_path ?? null, change: file.kind };
}

/**
 * What is read: the whole patch, one file, or nothing.
 * @param {readonly CommitFile[]} files the commit's files, in the list's order
 * @param {string | null} chosen the path the person picked, if any
 * @param {string} view the patch view chosen for every patch
 * @returns {CommitFocus | null}
 */
export function focusOf(files, chosen, view) {
  const list = Array.isArray(files) ? files : [];
  const picked = chosen === null ? undefined : list.find((f) => f.path === chosen);
  if (picked) return fileFocus(picked);
  if (!needsSides(view)) return { kind: "all" };
  return list.length ? fileFocus(list[0]) : null;
}

/**
 * The choice after a click on a row: the row's file, or nothing when the
 * chosen row is clicked again on Hunks — where letting go means the whole
 * patch. A comparison always reads one file, so its chosen row stays.
 * @param {string | null} chosen @param {string} path @param {string} view
 * @returns {string | null}
 */
export function chooseFile(chosen, path, view) {
  if (chosen === path && !needsSides(view)) return null;
  return path;
}

/**
 * Whether a row reads as the one in hand — the chosen file, or the file a
 * comparison fell back to.
 * @param {CommitFocus | null} focus @param {string} path
 */
export function isFocused(focus, path) {
  return focus?.kind === "file" && focus.path === path;
}

/**
 * The sentence over a patch with no lines in it: a moved file that did not
 * change, a change without lines, or a commit that changed no text.
 * @param {CommitFocus | null} focus
 */
export function emptyPatchWords(focus) {
  if (focus?.kind === "file") {
    if (focus.change === "renamed") return t("work-commit-focus-renamed-without-changes-file-moved-lines");
    if (focus.change === "copied") return t("work-commit-focus-copied-without-changes-file-copied-lines");
    return t("work-commit-focus-no-lines-changed-mode-empty-file");
  }
  return t("work-commit-focus-no-textual-change-against-first-parent");
}

/** What the list's header says a click does, under the view chosen. @param {string} view */
export function listHint(view) {
  return needsSides(view) ? t("work-commit-focus-pick-file-compare-two-versions") : t("work-commit-focus-pick-file-read-patch-alone-whole");
}

/** The line over a commit's patch the node cut where its read stops: the list above it is whole. */
export function cutWords() {
  return t("work-commit-focus-patch-cut-file-list-complete");
}
