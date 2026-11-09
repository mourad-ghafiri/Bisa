/**
 * A person's edit of a goal's own record — its title, statement and tags —
 * in the Details pane: whether the goal takes an edit, what the form starts
 * from, what it refuses, and the body `PATCH /goals/{id}` is sent.
 *
 * The node's rules, said before the request rather than after: a closed goal
 * takes no edit; the statement is fixed while a run is live — work is under
 * way against the words it was given — and is never blank. Only what changed
 * is sent, so a title renamed mid-run never carries the statement with it.
 *
 * Plain `.mjs` with a `.d.mts` beside it: `GoalInspector.tsx` draws the form
 * and `node --test` reaches the rules here.
 */

import { t } from "../../i18n/l10n.mjs";

/** Whether the run is live: started or queued to, and neither finished nor cancelled. */
function runLive(run) {
  return !!run && run.outcome == null && !run.cancelled;
}

/**
 * Whether the goal's record may be edited, and whether its statement may:
 * `can` is false on a closed goal; `statementLocked` while a run is live,
 * with `why` the sentence the form says under the field.
 * @returns {{can: boolean, statementLocked: boolean, why: string | null}}
 */
export function editable(goal, run) {
  if (!goal || goal.closed) return { can: false, statementLocked: true, why: null };
  const locked = runLive(run);
  return { can: true, statementLocked: locked, why: locked ? t("work-goal-edit-statement-fixed-while-run-live") : null };
}

/** What the form starts from: the goal's own words. */
export function draftOf(goal) {
  return {
    title: String(goal?.title ?? ""),
    statement: String(goal?.statement ?? ""),
    tags: [...(goal?.tags ?? [])],
  };
}

/** Why the draft cannot be saved, or null: a goal is never without a statement. */
export function problemOf(draft) {
  return String(draft?.statement ?? "").trim() === "" ? t("work-goal-edit-statement-required") : null;
}

function sameTags(a, b) {
  return a.length === b.length && a.every((tag, i) => tag === b[i]);
}

/**
 * The body of the edit: only the fields the draft changed — a title trimmed
 * (blank clears it), a statement trimmed, the tags whole — or null when
 * nothing changed and there is nothing to send.
 * @returns {{title?: string, statement?: string, tags?: string[]} | null}
 */
export function editBody(goal, draft) {
  const was = draftOf(goal);
  const body = {};
  const title = String(draft?.title ?? "").trim();
  if (title !== was.title.trim()) body.title = title;
  const statement = String(draft?.statement ?? "").trim();
  if (statement !== was.statement.trim()) body.statement = statement;
  const tags = [...(draft?.tags ?? [])];
  if (!sameTags(tags, was.tags)) body.tags = tags;
  return Object.keys(body).length > 0 ? body : null;
}

/** Whether Save is offered: something changed, and nothing is wrong with it. */
export function canSave(goal, draft) {
  return problemOf(draft) === null && editBody(goal, draft) !== null;
}
