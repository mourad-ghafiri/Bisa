/** Types for `conflictBlocksModel.mjs`, plain JavaScript so `node --test` reads it. */

export type Choice = "mine" | "theirs" | "both" | "both_reversed" | "edit";
export interface TextRun {
  kind: "text";
  text: string;
}
export interface ConflictBlock {
  kind: "conflict";
  id: string;
  /** git's ours — the current branch, or under a rebase the branch rebased onto. */
  ours: string;
  base: string | null;
  theirs: string;
  /** The conflict as git wrote it, markers included. */
  raw: string;
}
export type Segment = TextRun | ConflictBlock;
export interface Made {
  choice: Choice;
  text?: string;
}
export type Choices = Readonly<Record<string, Made>>;

export declare const CHOICES: readonly Choice[];
export declare const FOLD_UNDER: number;
export declare function parseConflicts(text: string | null | undefined): { segments: Segment[]; problem: string | null };
export declare function conflictsOf(segments: readonly Segment[] | null | undefined): ConflictBlock[];
export declare function hasMarkers(text: string | null | undefined): boolean;
export declare function textFor(conflict: ConflictBlock, made: Made | null | undefined, swapped: boolean): string;
export declare function compose(segments: readonly Segment[], choices: Choices | null | undefined, swapped: boolean): string;
export declare function choose(choices: Choices | null | undefined, id: string, choice: Choice, text?: string): Choices;
export declare function unchoose(choices: Choices | null | undefined, id: string): Choices;
export declare function chooseAll(segments: readonly Segment[], choice: Choice): Choices;
export declare function progress(segments: readonly Segment[], choices: Choices | null | undefined): { settled: number; total: number };
export declare function unsettledIds(segments: readonly Segment[], choices: Choices | null | undefined): string[];
export declare function nextUnsettled(segments: readonly Segment[], choices: Choices | null | undefined, from: string | null): string | null;
export declare function previousUnsettled(segments: readonly Segment[], choices: Choices | null | undefined, from: string | null): string | null;
export declare function lineCount(text: string | null | undefined): number;
export declare function foldWords(lines: number): string;
export declare function blockWords(made: Made | null | undefined, sides: { mine: { name: string }; theirs: { name: string } }): string;
export declare function documentWords(p: { settled: number; total: number }): string;
