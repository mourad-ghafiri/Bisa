/**
 * Types for `turnChangesModel.mjs`. This file is the only reason TypeScript
 * never has to read it.
 */

import type { ChangesView, FileChangeView, TurnChangesView } from "../../types";

export interface TurnCard {
  turn: string;
  agent: string;
  mode: string;
  files: FileChangeView[];
  words: string;
  pending: boolean;
  verbs: FileVerb[];
}

export declare const TIMELINE_END: string;

export type FileVerb = "keep" | "undo" | "undo_with_note";
export type ChipTone = "quiet" | "accent" | "warn";

export declare function turnWords(files: readonly FileChangeView[] | null | undefined): string;
export declare function fileChipWords(file: FileChangeView | null | undefined): string;
export declare function fileChipTone(file: FileChangeView | null | undefined): ChipTone;
export declare function fileVerbs(file: FileChangeView | null | undefined): FileVerb[];
export declare function turnHasPending(turn: TurnChangesView | null | undefined): boolean;
export declare function turnVerbs(turn: TurnChangesView | null | undefined): FileVerb[];
export declare function turnCardsByAnchor(
  view: ChangesView | null | undefined,
  timelineIds?: Iterable<string> | null,
): { byMessage: Map<string, TurnCard[]>; atEnd: TurnCard[] };
export declare function pendingPathsOf(view: ChangesView | null | undefined): Set<string>;
export declare function autoKeptHint(): string;
export declare function skippedWords(skipped: readonly { path: string; why: string }[] | null | undefined): string | null;
