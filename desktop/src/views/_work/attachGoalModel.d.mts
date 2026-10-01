/**
 * Types for `attachGoalModel.mjs`, which is plain JavaScript so `node --test`
 * can import it without a build step.
 */

/** One goal as the pickers list it. */
export interface GoalChoice {
  id: string;
  label: string;
}

/** The goals a project is not yet on — the dialog's options. */
export declare function attachableGoals(goals: readonly GoalChoice[], attached: readonly string[] | null | undefined): GoalChoice[];
/** The goal the select stands on: the pick while offered, else the only candidate, else nothing. */
export declare function attachChoice(candidates: readonly { id: string }[], picked?: string | null): string;
/** Why there is nothing to offer. */
export declare function attachEmptyWords(goalCount: number): string;
/** The toast: both ends named. */
export declare function attachedWords(project: string, goalLabel: string): string;
/** A goal's label, or the tail of its id when the list no longer has it. */
export declare function goalLabelOf(goals: readonly GoalChoice[], id: string): string;
