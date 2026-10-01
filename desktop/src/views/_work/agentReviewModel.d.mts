import type { ReviewEvent } from "./reviewStepModel.mjs";

export type ReviewTarget = { kind: "pr"; pr: { number: number; title: string }; noun?: string } | { kind: "branch"; base: string };
export declare function reviewPrompt(pr: { number: number; title: string }, noun?: string, message?: string): string;
export declare function branchReviewPrompt(base: string, message?: string): string;
export declare function askContent(target: ReviewTarget, message?: string): string;
export declare function askLabel(target: ReviewTarget, agent: string): string;
export declare function landsWords(target: ReviewTarget): string;
export declare const FIX_MENU_LABEL: string;
export declare function fixLabel(remembered: string): string;
export declare function fixAllLabel(n: number): string;
export declare function draftKeys(scope: string): { agent: string; message: string; run: string; fixAgent: string; checkAgent: string };
export declare const DEFAULT_AGENT: string;
export interface ReviewButton {
  event: ReviewEvent;
  label: string;
  primary: boolean;
  danger: boolean;
}
export declare function reviewButtons(allowed: readonly string[]): ReviewButton[];
export declare function wordsReason(event: string, body: string): string | null;
