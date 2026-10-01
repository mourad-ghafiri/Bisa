/**
 * The views of one changed file's patch in the centre (ide/04 §The Changes
 * view), as facts: **Hunks** — the patch hunk by hunk with the three acts,
 * stage a hunk, pick lines, annotate; **Side by side** — the two versions
 * in two columns, read-only; **Inline** — one column, removed and added
 * lines interleaved, read-only. The two comparisons need the file's two
 * whole texts (`GET /git/sides`, or a commit's `GET /git/commit/{sha}/sides`),
 * the hunks need the patch; the choice is one for every patch — a changed
 * file's and a commit's file alike (ide/05) — remembered on this machine
 * (`bisa.ide.views`, `patch`). Plain `.mjs`, so `node --test` reads it.
 */

import { t } from "../../i18n/l10n.mjs";

/** The views, in the control's order. */
export const PATCH_VIEWS = Object.freeze(["hunks", "split", "inline"]);
export const PATCH_VIEW_LABEL = Object.freeze({
  hunks: t("work-patch-view-hunks"),
  split: t("work-patch-view-side-side"),
  inline: t("work-patch-view-inline"),
});

/**
 * The glyph a view wears on the control, as a key of `ui/icons.ts`: a list
 * for the hunks, two columns for side by side, stacked rows for inline.
 * @param {"hunks" | "split" | "inline"} view
 * @returns {"list" | "splitRight" | "splitDown"}
 */
export function patchViewGlyph(view) {
  switch (view) {
    case "split":
      return "splitRight";
    case "inline":
      return "splitDown";
    default:
      return "list";
  }
}

/**
 * The control's tooltip for a view: what it shows, and that a comparison is
 * read-only — as are the hunks of a patch that is history (`readOnly`: a
 * commit's), where the acts do not apply.
 * @param {"hunks" | "split" | "inline"} view
 * @param {{readOnly?: boolean}} [facts]
 */
export function patchViewHint(view, facts = {}) {
  switch (view) {
    case "split":
      return t("work-patch-view-side-side-two-versions-two-columns");
    case "inline":
      return t("work-patch-view-inline-one-column-removed-added-lines");
    default:
      return facts.readOnly ? t("work-patch-view-hunks-hunk-hunk-read-only") : t("work-patch-view-hunks-hunk-hunk-acts-stage-pick");
  }
}

/** Whether a view draws a comparison of the two whole texts rather than the patch. @param {string} view */
export function needsSides(view) {
  return view === "split" || view === "inline";
}

/**
 * The sentence over a comparison, or null when the two texts speak for
 * themselves: a binary side has no text to compare; a side git does not
 * hold — a new file's left, a deleted file's right — is said; a side cut
 * at the cap is said.
 * @param {{original?: string | null, modified?: string | null, binary: boolean, truncated: boolean} | null | undefined} sides
 * @returns {string | null}
 */
export function sidesWords(sides) {
  if (!sides) return null;
  if (sides.binary) return t("work-patch-view-binary-file-no-text-compare");
  const notes = [];
  const left = sides.original ?? null;
  const right = sides.modified ?? null;
  if (left === null && right !== null) notes.push(t("work-patch-view-new-left-side-empty"));
  if (right === null && left !== null) notes.push(t("work-patch-view-deleted-right-side-empty"));
  if (sides.truncated) notes.push(t("work-patch-view-first-2-mib-each-side-shown"));
  return notes.length ? notes.join(" ") : null;
}
