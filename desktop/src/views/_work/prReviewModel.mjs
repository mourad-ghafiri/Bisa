/**
 * Pull-request reviews as facts: grouping the resolvable **comments** a
 * pull request carries (a reviewer's inline conversation on a line — the
 * code host's *thread* on the wire, "ReviewThread"; the desktop says
 * *comment*), counting what is still open, the words and tone for a review's
 * verdict, and the prompt that asks an agent to fix comments. The desktop's
 * "ReviewStep" draws these; "node --test" checks them.
 */

import { t as tr } from "../../i18n/l10n.mjs";

/** How many comments still want attention. @param {readonly {is_resolved:boolean}[]} comments */
export function unresolvedCount(comments) {
  return (comments ?? []).filter((t) => !t.is_resolved).length;
}

/**
 * Comments grouped by file, files with open ones first, each file's by line.
 * A comment with no path (a general one) files under "" and leads.
 * @param {readonly {path?:string|null, line?:number|null, is_resolved:boolean}[]} comments
 * @returns {{path: string, comments: object[], unresolved: number}[]}
 */
export function commentsByFile(comments) {
  const byPath = new Map();
  for (const t of comments ?? []) {
    const key = t.path ?? "";
    const list = byPath.get(key) ?? [];
    list.push(t);
    byPath.set(key, list);
  }
  const groups = [...byPath.entries()].map(([path, list]) => ({
    path,
    comments: [...list].sort((a, b) => (a.line ?? 0) - (b.line ?? 0)),
    unresolved: unresolvedCount(list),
  }));
  // Files with open comments first; then by path so the order is stable.
  return groups.sort((a, b) => Number(b.unresolved > 0) - Number(a.unresolved > 0) || a.path.localeCompare(b.path));
}

/** The word for a review's verdict. @param {string} state */
export function reviewStateLabel(state) {
  switch (state) {
    case "approved":
      return tr("work-pr-review-approved");
    case "changes_requested":
      return tr("work-pr-review-changes-requested");
    case "commented":
      return tr("work-pr-review-commented");
    case "dismissed":
      return tr("work-pr-review-dismissed");
    case "pending":
      return tr("work-pr-review-pending");
    default:
      return state;
  }
}

/** A chip tone for a review's verdict — a kit "Chip" tone. @param {string} state */
export function reviewTone(state) {
  switch (state) {
    case "approved":
      return "ok";
    case "changes_requested":
      return "danger";
    case "dismissed":
      return "quiet";
    default:
      return "neutral";
  }
}

/**
 * The prompt that asks an agent to fix review comments — all the open ones,
 * or the one handed to it. Each comment carries its thread id, and the agent
 * is told to answer on the thread itself — "pr_thread_reply", resolving what
 * it addressed — so the code host reads what happened without anyone
 * relaying it. (The prompts that ask for a review are "agentReviewModel.mjs"'s.)
 * @param {{number:number}} pr @param {readonly {id: string, path?:string|null, line?:number|null, is_resolved:boolean, comments:readonly {body:string}[]}[]} comments
 */
export function fixPrompt(pr, comments, noun = tr("work-publish-outcome-banner-pull-request")) {
  const open = (comments ?? []).filter((t) => !t.is_resolved);
  const lines = open.map((t) => {
    const where = t.path ? `${t.path}${t.line ? `:${t.line}` : ""}` : "general";
    const said = t.comments.map((c) => c.body).join(" / ");
    return `- ${where} [thread ${t.id}]: ${said}`;
  });
  // For the agent, never a person: English, as the model reads it.
  return [
    `Address the open review comments on ${noun} #${pr.number}, working in this checkout:`,
    ...lines,
    "Make the changes and commit them on this branch. Do not push or merge — the person decides that.",
    "When a comment is addressed, reply on its thread with `pr_thread_reply` — say what you changed and name the commit — with `resolve: true`, so the thread closes with your answer on it (`pr_thread_resolve` reopens or resolves on its own). Leave a comment you did not address open, and say why in a reply.",
  ].join("\n");
}

/**
 * The prompt that hands a failed check run to an agent: what failed, where
 * the log is, and the bounds — reproduce in this checkout, fix, commit, no
 * push or merge, report here.
 * @param {{number:number}} pr
 * @param {{name: string, conclusion?: string | null, summary?: string | null, url?: string | null}} check
 */
export function checkFixPrompt(pr, check, noun = tr("work-publish-outcome-banner-pull-request")) {
  const how = check.conclusion ? ` (${check.conclusion.replace(/_/g, " ")})` : "";
  // For the agent, never a person: English, as the model reads it.
  return [
    `Check "${check.name}" failed${how} on ${noun} #${pr.number}.`,
    ...(check.summary ? [`The code host says: ${check.summary}`] : []),
    ...(check.url ? [`Its log: ${check.url} — read it when the summary is not enough.`] : []),
    "Reproduce the failure in this checkout first, fix its cause, and commit on this branch. Do not push or merge — the person decides that.",
    "Reply here with what failed and what you changed.",
  ].join("\n");
}

/**
 * Whether a check run failed — the one definition, read by the Checks step's
 * *Fix with* and by "checksSummary": completed, and concluded as a failure,
 * a timeout, an action required, or a cancellation.
 * @param {{status: string, conclusion?: string | null}} check
 */
export function failedCheck(check) {
  return check.status === "completed" && FAILED.includes(check.conclusion ?? "");
}

const FAILED = Object.freeze(["failure", "timed_out", "action_required", "cancelled"]);
