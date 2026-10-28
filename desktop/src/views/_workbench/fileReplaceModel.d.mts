import type { IdeReplace, IdeReplaceOutcome, IdeReplacePreview } from "../../types";

export interface ReplaceQuestion {
  needle: string;
  include: readonly string[];
  exclude: readonly string[];
}
export interface ReplaceFlags {
  regex?: boolean;
  caseSensitive?: boolean;
  word?: boolean;
}
export interface ReplaceFile {
  path: string;
  base_hash: string;
}

export declare function replaceBody(query: ReplaceQuestion, flags: ReplaceFlags, replacement: string, plan?: readonly ReplaceFile[] | null): IdeReplace;
export declare function questionKey(query: ReplaceQuestion, flags: ReplaceFlags, replacement: string): string;
export declare function previewStale(preview: { key: string } | null | undefined, key: string): boolean;

export type PreviewRow =
  | { kind: "file"; key: string; path: string; count: number; baseHash: string }
  | { kind: "change"; key: string; path: string; line: number; before: string; after: string }
  | { kind: "more"; key: string; path: string; more: number };
export declare function previewRows(files: readonly IdeReplacePreview[], perFile?: number): PreviewRow[];
export declare function replacePlan(files: readonly ReplaceFile[], only?: string | null): ReplaceFile[] | null;
export declare function previewWords(files: readonly { changes: readonly unknown[] }[]): string;

export interface ReplaceOutcomeWords {
  tone: "ok" | "warn" | "danger";
  text: string;
  applied: number;
  skipped: number;
  failed: number;
}
export declare function replaceOutcomeWords(outcomes: readonly IdeReplaceOutcome[]): ReplaceOutcomeWords;
export declare function captureHint(regex: boolean): string;
