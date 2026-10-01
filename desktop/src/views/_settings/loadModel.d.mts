/**
 * Types for `loadModel.mjs`, which is plain JavaScript so `node --test` can
 * import it without a build step.
 */

export type Phase = "pending" | "failed" | "ready";

export interface Read {
  data?: unknown;
  loading?: boolean;
  error?: string | null;
  refreshing?: boolean;
  at?: number | null;
}

export declare function phase(read: Read): Phase;
export declare function phaseOf(reads: readonly Read[]): Phase;
export declare function firstFailure(reads: readonly Read[]): string | null;
export declare function agoWords(seconds: number): string;
export declare function readWords(read: Read & { what: string }, now?: number): { text: string; failed: boolean } | null;
export declare const PENDING_ROWS: Readonly<Record<string, number>>;
export declare function pendingRows(what: string): number;
