/**
 * Types for `hunkModel.mjs`, which is plain JavaScript so `node --test` can
 * import it without a build step.
 */

import type { DiffScope, ReviewNote } from "../../types";

export type HunkLineKind = "context" | "add" | "del" | "meta";

export interface HunkLine {
  kind: HunkLineKind;
  /** The raw line, prefix included. */
  text: string;
  /** 1-based line in the old file; `null` for an addition or a marker. */
  oldLine: number | null;
  /** 1-based line in the new file; `null` for a deletion or a marker. */
  newLine: number | null;
}

export interface Hunk {
  /** Position in the diff, 0-based. */
  index: number;
  /** The `@@` line exactly as git wrote it. */
  header: string;
  oldStart: number;
  oldCount: number;
  newStart: number;
  newCount: number;
  /** What followed the second `@@` — usually the enclosing function. */
  context: string;
  lines: HunkLine[];
  /** The header and every line, newline-terminated — what a note hashes. */
  text: string;
}

export function parseHunks(diff: string): { header: string; hunks: Hunk[] };
export function hunkPatch(header: string, hunk: Hunk): string;
export function linesPatch(header: string, hunk: Hunk, picked: Iterable<number>): string | null;
export function pickable(hunk: Hunk): number[];
export function hunkRange(hunk: Hunk): { start: number; end: number };
export function noteScope(staged: boolean): { scope: "staged" } | { scope: "unstaged" };
export function scopeLabel(scope: DiffScope): string;
export function unsentNotes(notes: ReviewNote[]): ReviewNote[];
/** Whether a refused write of a review note means the note is gone (the node's 404). */
export function reviewNoteGone(status: number | null | undefined): boolean;
/** *hunk 2 of 5* — where a hunk stands among the patch's. */
export declare function hunkPosition(index: number, total: number): string;
