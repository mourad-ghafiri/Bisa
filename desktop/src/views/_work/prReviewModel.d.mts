import type { CheckRun, ReviewThread } from "../../types";

/** A reviewer's inline conversation on a pull request — the code host's `ReviewThread`; the desktop's word is *comment*. */
export type ReviewComment = ReviewThread;

export declare function unresolvedCount(comments: readonly ReviewComment[]): number;
export declare function commentsByFile(
  comments: readonly ReviewComment[],
): { path: string; comments: ReviewComment[]; unresolved: number }[];
export declare function reviewStateLabel(state: string): string;
export declare function reviewTone(state: string): "ok" | "danger" | "quiet" | "neutral";
export declare function fixPrompt(
  pr: { number: number },
  comments: readonly ReviewComment[],
  noun?: string,
): string;
export declare function checkFixPrompt(
  pr: { number: number },
  check: Pick<CheckRun, "name" | "conclusion" | "summary" | "url">,
  noun?: string,
): string;
/** Whether a check run failed — completed as a failure, a timeout, an action required, or a cancellation. */
export declare function failedCheck(check: Pick<CheckRun, "status" | "conclusion">): boolean;
