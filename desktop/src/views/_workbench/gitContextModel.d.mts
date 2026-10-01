/**
 * Types for `gitContextModel.mjs`. This file is the only reason TypeScript
 * never has to read it.
 */

import type { ContextRef } from "../../types";
import type { Hunk } from "../_work/hunkModel.mjs";

export interface ChangedRow {
  path: string;
  untracked: boolean;
}

export interface FilePatches {
  staged?: string | null;
  unstaged?: string | null;
}

export declare function hunkId(path: string, hunk: Pick<Hunk, "newStart">): string;
export declare function fileChips(row: ChangedRow, patches: FilePatches): ContextRef[];
export declare function changeChips(rows: readonly ChangedRow[], patches: ReadonlyMap<string, FilePatches>): ContextRef[];
export declare function attachWithin(
  existing: readonly ContextRef[],
  chips: readonly ContextRef[],
): { tray: ContextRef[]; attached: ContextRef[]; leftOut: number };
export declare function attachedWords(attached: readonly ContextRef[], leftOut: number): string;
