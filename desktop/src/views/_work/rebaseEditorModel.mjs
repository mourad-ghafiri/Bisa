/**
 * The interactive rebase editor's rules (ide/04 §The Branches view): the
 * commits since the target as rows, oldest first — the order git replays
 * them — each with an action, reordered by moving a row, with the message a
 * reword or a squash carries; the problems that keep the button off, said
 * as sentences the vcs crate checks again; the summary line; the plan git
 * gets. Pure, so `node --test` reads it; `RebaseEditorDialog.tsx` draws.
 */

import { VERB } from "./gitWords.mjs";
import { t } from "../../i18n/l10n.mjs";

/** The actions, in the segment's order, each with its word and its sentence. */
export const REBASE_ACTIONS = Object.freeze(["pick", "reword", "squash", "fixup", "drop"]);

export const REBASE_ACTION_WORDS = Object.freeze({
  pick: Object.freeze({ label: t("work-rebase-editor-pick"), meaning: t("work-rebase-editor-keep-commit") }),
  reword: Object.freeze({ label: t("work-rebase-editor-reword"), meaning: t("work-rebase-editor-keep-new-message") }),
  squash: Object.freeze({ label: t("work-rebase-editor-squash"), meaning: t("work-rebase-editor-fold-into-commit-above-keeping-both") }),
  fixup: Object.freeze({ label: t("work-rebase-editor-fixup"), meaning: t("work-rebase-editor-fold-into-commit-above-keeping-only") }),
  drop: Object.freeze({ label: t("work-rebase-editor-drop"), meaning: t("work-rebase-editor-leave-out") }),
});

/**
 * The editor's rows from the commits the branch has since the target —
 * the listing is newest first, the rows are oldest first, as git replays.
 * @param {readonly {id: string, short: string, subject: string, author?: string}[]} commits newest first
 */
export function editorRows(commits) {
  return [...commits].reverse().map((c) => ({ id: c.id, short: c.short, subject: c.subject, author: c.author ?? "", action: "pick", message: c.subject }));
}

/** The row moved one place up or down; the ends hold. */
export function moveStep(rows, i, dir) {
  const j = dir === "up" ? i - 1 : i + 1;
  if (i < 0 || i >= rows.length || j < 0 || j >= rows.length) return rows;
  const out = [...rows];
  [out[i], out[j]] = [out[j], out[i]];
  return out;
}

/** The commit kept above row `i` — the one a squash or fixup folds into. */
function keptAbove(rows, i) {
  for (let k = i - 1; k >= 0; k--) if (rows[k].action !== "drop") return rows[k];
  return null;
}

/**
 * A row's action changed. A squash composes its message from the commit
 * above's and its own, as git would, for the person to edit; a reword keeps
 * the message it has; the rest do not read the message.
 */
export function setAction(rows, i, action) {
  const out = rows.map((r) => ({ ...r }));
  const row = out[i];
  if (!row) return rows;
  row.action = action;
  if (action === "squash") {
    const above = keptAbove(out, i);
    row.message = above ? `${above.message.trim()}\n\n${row.subject}` : row.subject;
  }
  return out;
}

/** A row's message, as typed. */
export function setMessage(rows, i, message) {
  const out = rows.map((r) => ({ ...r }));
  if (out[i]) out[i].message = message;
  return out;
}

/**
 * The first thing that keeps the plan from running, as a sentence, or
 * `null` — the same rules the vcs crate holds the plan to.
 */
export function planProblem(rows) {
  if (rows.length === 0) return t("work-rebase-editor-there-nothing-rebase");
  const kept = rows.filter((r) => r.action !== "drop");
  if (kept.length === 0) return t("work-rebase-editor-every-commit-dropped-nothing-would-left");
  if (kept[0].action === "squash" || kept[0].action === "fixup") return t("work-rebase-editor-cannot-fold-first-kept", { short: kept[0].short });
  const wordless = rows.find((r) => r.action === "reword" && r.message.trim() === "");
  if (wordless) return t("work-rebase-editor-reworded-without-message", { short: wordless.short });
  return null;
}

/** *5 commits → 3: one reworded, two squashed, one dropped*. */
export function planSummary(rows) {
  const n = rows.length;
  const kept = rows.filter((r) => r.action === "pick" || r.action === "reword").length;
  const say = (k) => ["no", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine"][k] ?? String(k);
  const parts = [];
  const c = (action, word) => {
    const k = rows.filter((r) => r.action === action).length;
    if (k > 0) parts.push(`${say(k)} ${word}`);
  };
  c("reword", "reworded");
  c("squash", "squashed");
  c("fixup", t("work-rebase-editor-fixed-up"));
  c("drop", "dropped");
  const head = t("work-rebase-editor-commit-commits", { n, kept });
  return parts.length === 0 ? t("work-rebase-editor-unchanged", { head }) : `${head}: ${parts.join(", ")}`;
}

/** Whether the plan would change anything at all. */
export function planIsIdentity(rows, commits) {
  const original = [...commits].reverse().map((c) => c.id);
  return rows.every((r, i) => r.action === "pick" && r.id === original[i]);
}

/** The plan the node takes — the rows in order, a message only where one is read. */
export function planOf(rows, upstream, onto = null) {
  return {
    upstream,
    onto,
    steps: rows.map((r) => ({
      action: r.action,
      commit: r.id,
      message: r.action === "reword" || (r.action === "squash" && r.message.trim() !== "") ? r.message : null,
    })),
  };
}

/** The confirmation before the plan runs. */
export function planConsent(current, onto, summary) {
  return { title: t("work-rebase-editor-interactively-onto", { rebase: VERB.rebase, current, onto }), body: t("work-rebase-editor-conflict-stops-rebase-here-card-above", { summary }), confirm: VERB.rebase, danger: false, kind: "tree" };
}
