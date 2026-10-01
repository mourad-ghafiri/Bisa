/**
 * Types for `newGoalModel.mjs`, which is plain JavaScript so `node --test`
 * can import it without a build step.
 */

import type { AttachmentRef, GoalMode } from "../../types";

export { captureToast } from "../_goal/goalMode.mjs";

export interface GoalForm {
  statement: string;
  mode: GoalMode;
  /** A team handed over by the Teams screen; never picked here. */
  assignees: string[];
  tags: string[];
  documents: AttachmentRef[];
}

export interface GoalBody {
  statement: string;
  mode: GoalMode;
  assignees?: string[];
  tags?: string[];
  documents?: AttachmentRef[];
}

export declare function goalBody(form: GoalForm): GoalBody;
export declare function canSubmit(s: { statement: string; busy: boolean; uploading: boolean }): boolean;
export declare function captureLabel(s: { busy: boolean; uploading: boolean }): string;
export declare function documentsHint(count: number): string;

/** Who a capture is handed to, on the wire: the team handed over, or nobody. */
export declare function captureAssignees(team: string | null | undefined): string[];
/** What is said when projects handed over with a capture could not be attached; `null` when every one was. */
export declare function attachRefusedWords(refused: readonly { project: string; reason: string }[]): string | null;
