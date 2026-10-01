/** Types for `conflictSidesModel.mjs`, plain JavaScript so `node --test` reads it. */

import type { GitConflictKind, GitInProgress, GitOperationFacts, GitResolution } from "../../types";

export interface Side {
  key: "mine" | "theirs";
  /** git's word for it — the swap made once, here. */
  git: "ours" | "theirs";
  name: string;
  role: string;
  tone: string;
}
export interface Sides {
  mine: Side;
  theirs: Side;
  explain: string;
  step: string | null;
  title: string;
}
export interface KindChoice {
  id: string;
  label: string;
  hint: string;
  take: GitResolution;
  danger: boolean;
}

export declare const SIDE_TONE: { readonly mine: string; readonly theirs: string };
export declare function gitSideOf(inProgress: GitInProgress | null | undefined, side: "mine" | "theirs"): "ours" | "theirs";
export declare function isSwapped(inProgress: GitInProgress | null | undefined): boolean;
export declare function sidesOf(inProgress: GitInProgress | null | undefined, facts: GitOperationFacts | null | undefined): Sides;
export declare function sideByGit(sides: Sides, git: "ours" | "theirs"): Side;
export declare function kindWords(kind: GitConflictKind | null | undefined, sides: Sides): { short: string; sentence: string };
export declare function isWholeFileKind(kind: GitConflictKind | null | undefined): boolean;
export declare function kindChoices(kind: GitConflictKind | null | undefined, sides: Sides): KindChoice[];
export declare function whatIsAConflict(inProgress: GitInProgress | null | undefined): string[];
export declare function tookWords(path: string, take: GitResolution, sides: Sides): string;
export declare function agentQuestion(path: string, conflict: { ours: string; theirs: string; base: string | null }, sides: Sides, where: { at: number; of: number }): { text: string; question: string };
