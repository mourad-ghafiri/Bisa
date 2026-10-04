/**
 * Types for `newGoalModel.mjs`, which is plain JavaScript so `node --test`
 * can import it without a build step.
 */

import type { AttachmentRef, GoalMode } from "../../types";

export { captureToast } from "../_goal/goalMode.mjs";

export interface GoalForm {
  statement: string;
  mode: GoalMode;
  /** Who carries the goal: the agents and teams picked, in the wire's word (`agent:<id>`, `team:<id>`). */
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
/** The agents *Who carries it* never offers: core and disabled ones. */
export declare function staffNotOffered(agents: readonly { id: string; origin: unknown; enabled: boolean }[]): string[];
/** What *Who carries it* says under itself, by how many are picked. */
export declare function staffHint(count: number): string;

/** What is said when projects handed over with a capture could not be attached; `null` when every one was. */
export declare function attachRefusedWords(refused: readonly { project: string; reason: string }[]): string | null;
