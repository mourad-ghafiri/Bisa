/**
 * The checkout's changes as context chips (ide/09) — on request, never on
 * their own. A changed file becomes the chips the contract already has: one
 * `diff_hunk` per hunk, the staged side's before the unstaged side's, each
 * with the id a dragged hunk carries so the two are the same chip; an
 * untracked file, which has no patch, becomes a `file` chip the agent reads.
 * What is attached stays inside the 64 KiB budget: the longest prefix that
 * fits goes in, the rest is counted and said.
 *
 * Plain JavaScript so `node --test` runs it; `gitContext.ts` fetches and
 * attaches, the three doors call that.
 */

import { parseHunks } from "../_work/hunkModel.mjs";
import { attach, fileChip, fitsBudget, hunkChip } from "./contextChips.mjs";
import { t } from "../../i18n/l10n.mjs";

/** The id a hunk chip carries — the same one `hunkDrag` gives it (`HunkDiff.tsx`). */
export function hunkId(path, hunk) {
  return `${path}@${hunk.newStart}`;
}

/**
 * One file's chips from its patches: the staged patch's hunks, then the
 * unstaged patch's; an untracked file is a file chip. A patch with no hunks
 * yields nothing.
 * @param {{path: string, untracked: boolean}} row
 * @param {{staged?: string | null, unstaged?: string | null}} patches
 */
export function fileChips(row, patches) {
  if (row.untracked) return [fileChip(row.path)];
  const out = [];
  for (const [staged, patch] of [[true, patches.staged], [false, patches.unstaged]]) {
    if (!patch) continue;
    for (const h of parseHunks(patch).hunks) out.push(hunkChip(row.path, staged, h.text, hunkId(row.path, h)));
  }
  return out;
}

/**
 * Every chip for the changed files, in the listing's order.
 * @param {{path: string, untracked: boolean}[]} rows
 * @param {Map<string, {staged?: string | null, unstaged?: string | null}>} patches by path
 */
export function changeChips(rows, patches) {
  return rows.flatMap((row) => fileChips(row, patches.get(row.path) ?? {}));
}

/**
 * The tray after attaching as many of `chips` as fit, in order, each
 * replacing an earlier chip for the same thing: the chips that went in and
 * how many were left out over the budget.
 * @param {import("../../types").ContextRef[]} existing
 * @param {import("../../types").ContextRef[]} chips
 */
export function attachWithin(existing, chips) {
  let tray = [...existing];
  const attached = [];
  let leftOut = 0;
  for (const chip of chips) {
    if (leftOut > 0) {
      leftOut += 1;
      continue;
    }
    const next = attach(tray, chip);
    if (fitsBudget(next)) {
      tray = next;
      attached.push(chip);
    } else {
      leftOut += 1;
    }
  }
  return { tray, attached, leftOut };
}

/** *Attached 5 hunks from 3 files — 2 left out: over the 64 KiB context budget.* */
export function attachedWords(attached, leftOut) {
  const hunks = attached.filter((c) => c.kind === "diff_hunk").length;
  // *from N files* describes the hunks; an untracked file is a file of its own.
  const files = new Set(attached.filter((c) => c.kind === "diff_hunk").map((c) => c.path)).size;
  const parts = [];
  if (hunks > 0) parts.push(t("workbench-git-context-hunk-hunks", { hunks }));
  const plain = attached.length - hunks;
  if (plain > 0) parts.push(t("workbench-git-context-untracked-file-files", { plain }));
  const what = parts.length === 2 ? t("workbench-git-context-and", { a: parts[0], b: parts[1] }) : (parts[0] ?? t("workbench-git-context-nothing"));
  const from = hunks > 0 ? ` ${t("workbench-git-context-from-file-files", { files })}` : "";
  const cut = leftOut > 0 ? ` ${t("workbench-git-context-left-out-over-64-kib-context", { leftOut })}` : "";
  if (attached.length === 0 && leftOut === 0) return t("workbench-git-context-nothing-attach-checkout-has-no-changes");
  return t("workbench-git-context-attached", { what, from, cut });
}
