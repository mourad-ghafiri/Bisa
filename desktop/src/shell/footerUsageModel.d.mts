/**
 * Types for `footerUsageModel.mjs`, which is plain JavaScript so `node --test`
 * can import it without a build step.
 */

import type { UsageWords } from "./harnessUsageModel.mjs";

export declare function pickerRows<R extends { id: string; installed: boolean }>(rows: readonly R[]): R[];
export declare function pinnedHarness(pinned: string | null | undefined, rows: readonly { id: string; installed: boolean }[]): string | null;
export declare function usageStatWords(label: string, words: Pick<UsageWords, "meters" | "inline" | "note">): string;
