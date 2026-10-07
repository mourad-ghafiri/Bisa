/**
 * Types for `sessionCountsModel.mjs`, which is plain JavaScript so `node --test`
 * can import it without a build step.
 */

import type { SessionRow } from "../types";
import type { TerminalSessionState } from "./terminalsModel.mjs";

export declare const OFF_CHECKOUT_KINDS: readonly string[];
export declare function isHarnessProcess(row: Partial<SessionRow> | null | undefined, claimed: Map<string, string>): boolean;
export declare function harnessRows(sessions: readonly SessionRow[] | null | undefined, terminals: readonly TerminalSessionState[] | null | undefined): SessionRow[];
export declare function liveRows(sessions: readonly SessionRow[] | null | undefined): SessionRow[];
export declare function processRows(sessions: readonly SessionRow[] | null | undefined): SessionRow[];
export declare function scopeOf(row: Partial<SessionRow> | null | undefined): string | null;
export declare function isBusy(row: Partial<SessionRow> | null | undefined): boolean;
export declare function busyScopes(sessions: readonly SessionRow[] | null | undefined): Set<string>;
export declare function workingCount(sessions: readonly SessionRow[] | null | undefined, working: Readonly<Record<string, readonly string[]>> | null | undefined): number;
export declare function sessionCounts(
  sessions: readonly SessionRow[] | null | undefined,
  terminals: readonly TerminalSessionState[] | null | undefined,
  working?: Readonly<Record<string, readonly string[]>> | null,
): { live: number; harnesses: number; processes: number; working: number };
