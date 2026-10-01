/**
 * The attach dialog's decisions, with no React and no DOM in them.
 *
 * Attaching a project the workspace already has to a goal is one relation
 * written once (`POST /projects/{pid}/attach`): additive, reversible, nothing
 * on disk moves. The dialog that does it (`AttachGoalDialog.tsx`) is opened
 * from the project's About › Goals and from the rail's project menu — and
 * from nowhere else: creating a project asks nothing about goals. What can be
 * wrong here is data — which goals to offer, which one to choose for the
 * person, what to say when there is nothing to offer, how the toast names
 * both ends — so it lives here, where `node --test` reaches it, and the
 * component is paint.
 */

import { t } from "../../i18n/l10n.mjs";

/**
 * The goals a project is not yet on, in the workspace's order — the dialog's
 * options. A goal the project already has is not offered again: the node
 * would take it (`INSERT OR IGNORE`) and nothing would change, which is a
 * click that says something and does nothing.
 * @param {readonly {id: string, label: string}[]} goals every goal of the workspace
 * @param {readonly string[] | null | undefined} attached the goals the project is on
 */
export function attachableGoals(goals, attached) {
  const on = new Set(attached ?? []);
  return goals.filter((g) => !on.has(g.id));
}

/**
 * The goal the select stands on: the person's pick while it is still
 * offered; else the only candidate, chosen for them — there is nothing to
 * decide; else nothing — several goals are an explicit choice, since
 * attaching is one click with no confirmation after it.
 * @param {readonly {id: string}[]} candidates what `attachableGoals` offers
 * @param {string | null | undefined} [picked] the goal the person chose, if any
 * @returns {string} a goal id, or `""` for no choice yet
 */
export function attachChoice(candidates, picked = null) {
  if (picked && candidates.some((c) => c.id === picked)) return picked;
  return candidates.length === 1 ? candidates[0].id : "";
}

/**
 * Why there is nothing to offer, said instead of an empty select: the
 * workspace has no goal yet, or every goal already has this project.
 * @param {number} goalCount how many goals the workspace has at all
 */
export function attachEmptyWords(goalCount) {
  return goalCount === 0 ? t("work-attach-goal-dialog-no-goals-yet-capture-one") : t("work-attach-goal-dialog-every-goal-already-has-project");
}

/**
 * The toast once the record is written — both ends named, as the guide
 * promises: *web-app is attached to Dark mode.*
 * @param {string} project the project's name
 * @param {string} goalLabel the goal's label
 */
export function attachedWords(project, goalLabel) {
  return t("work-attach-goal-dialog-attached", { project, goal: goalLabel });
}

/**
 * A goal's label among the workspace's — the tail of its id when the list
 * has moved on and no longer has it, so a sentence never shows nothing.
 * @param {readonly {id: string, label: string}[]} goals
 * @param {string} id
 */
export function goalLabelOf(goals, id) {
  return goals.find((g) => g.id === id)?.label ?? id.slice(-6);
}
