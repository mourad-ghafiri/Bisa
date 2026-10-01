/**
 * Types for `loadingModel.mjs`, which is plain JavaScript so `node --test`
 * can import it without a build step.
 */

export declare const INDICATOR_DELAY_MS: number;
export declare function indicatorDue(mountedAt: number, now: number, delayMs?: number): boolean;
export declare function beatMs(inSurface: boolean): number;
export declare function placeholderFill(due: boolean): string;
