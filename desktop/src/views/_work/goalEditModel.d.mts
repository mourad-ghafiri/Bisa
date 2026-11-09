/**
 * Types for `goalEditModel.mjs`. This file is the only reason TypeScript
 * never has to read it.
 */

import type { Goal, GoalEditBody } from "../../types";

export interface GoalDraft {
  title: string;
  statement: string;
  tags: string[];
}
type RunLike = { outcome?: unknown; cancelled?: unknown } | null | undefined;
/** Whether the goal's record may be edited, and whether its statement may — with the sentence that says why not. */
export declare function editable(goal: Goal | null | undefined, run: RunLike): { can: boolean; statementLocked: boolean; why: string | null };
export declare function draftOf(goal: Goal | null | undefined): GoalDraft;
/** Why the draft cannot be saved, or null. */
export declare function problemOf(draft: GoalDraft | null | undefined): string | null;
/** The body of the edit — only what changed — or null when nothing did. */
export declare function editBody(goal: Goal | null | undefined, draft: GoalDraft | null | undefined): GoalEditBody | null;
export declare function canSave(goal: Goal | null | undefined, draft: GoalDraft | null | undefined): boolean;
