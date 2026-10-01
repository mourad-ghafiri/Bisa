/**
 * Types for `reviewLensModel.mjs`. This file is the only reason TypeScript
 * never has to read it.
 */

import type { Hunk, SettleTarget } from "../../types";

export declare function lensStorageKey(rootKey: string, path: string): string;
export declare function lensLayoutKey(rootKey: string, path: string): string;
export type LensLayout = "inline" | "side-by-side";
export declare const LENS_LAYOUTS: readonly LensLayout[];
export declare const DEFAULT_LENS_LAYOUT: LensLayout;
export declare function toggleLensLayout(layout: LensLayout | string): LensLayout;
export declare function lensLayoutWords(layout: LensLayout | string): string;
export declare function reviewOpenDrafts(rootKey: string, path: string): { modeKey: string; mode: "source"; lensKey: string; lensOn: true };
export declare function pendingReviewWords(n: number): { text: string; show: string };
export declare function hasPreviousHunk(hunks: readonly Hunk[] | null | undefined, index: number): boolean;
export declare function hasNextHunk(hunks: readonly Hunk[] | null | undefined, index: number): boolean;
export declare function previousHunkIndex(hunks: readonly Hunk[] | null | undefined, index: number): number;
export declare function nextHunkIndex(hunks: readonly Hunk[] | null | undefined, index: number): number;
export declare function hunkPositionWords(hunks: readonly Hunk[] | null | undefined, index: number): string;
export declare function undoEnabled(dirty: boolean): boolean;
export declare const UNDO_DIRTY_HINT: string;
export declare function hunkActionWords(): { keep: string; undo: string };
export declare function fileActionWords(left?: number): { keepFile: string; undoFile: string; nextFile: string; dropLens: string };
export declare function lensApplies(file: unknown): boolean;
export declare function hunkSettleTarget(path: string, hunk: Pick<Hunk, "id">, diskHash: string): SettleTarget;
export declare function fileSettleTarget(path: string): SettleTarget;
