import type { UsageReport, UsageState } from "../types";

export type UsageToneWord = "ok" | "warn" | "danger";
export interface Meter {
  id: string;
  label: string;
  percent: number;
  tone: UsageToneWord;
  scope: string | null;
  /** This window's own reset words, or null when the harness does not know. */
  resets: string | null;
  /** Its tooltip: *Weekly 41% · resets in 3 d*. */
  tip: string;
}
export interface UsageWords {
  /** The windows, in the adapter's order — the compact line. */
  meters: Meter[];
  /** The credit balance, apart — the full line only. */
  extras: Meter[];
  /** The first window's reset, the one the line says inline. */
  inline: string | null;
  note: string | null;
  tone: UsageToneWord | "quiet";
}
export interface Settled {
  state: UsageState;
  readAt: number;
  stale: string | null;
}

export declare const USAGE_KEEP_S: number;
export declare const USAGE_RETRY_MS: readonly number[];
/** Milliseconds until a failed read with nothing kept is asked again, or null when it answered or the backoff is spent. */
export declare function retryDelay(entry: { state: UsageState | null; stale: string | null }, attempt: number): number | null;
export declare function usageKey(harness: string): string;
export declare function usageTone(percent: number): UsageToneWord;
export declare function resetWords(resetsAt: number | null | undefined, now?: number): string | null;
export declare function usageWords(state: UsageState | null | undefined, now?: number): UsageWords;
export declare function usageTitle(report: UsageReport, now?: number, stale?: string | null): string;
export declare function settled(previous: { state: UsageState | null; readAt: number }, answer: UsageState, now: number): Settled;
