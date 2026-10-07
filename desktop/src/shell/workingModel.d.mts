/**
 * Types for `workingModel.mjs`, which is plain JavaScript so `node --test`
 * can import it without a build step.
 */

import type { SessionRow } from "../types";

export type Working = Record<string, string[]>;

export declare function withThinking(working: Working, scope: string | null | undefined, agent: string | null | undefined): Working;
export declare function withReplied(working: Working, scope: string | null | undefined, agent: string | null | undefined): Working;
export declare function clearedByRow(working: Working, row: Partial<SessionRow> | null | undefined): Working;
export declare function rebuilt(rows: readonly SessionRow[] | null | undefined): Working;
export declare function sameWorking(a: Readonly<Working> | null | undefined, b: Readonly<Working> | null | undefined): boolean;
