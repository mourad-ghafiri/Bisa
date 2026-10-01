/** Types for `format.mjs`. */

export declare function number(n: number, options?: Intl.NumberFormatOptions): string;
export declare function percent(ratio: number, digits?: number): string;
export declare function bytes(bytes: unknown): string;
export declare function dateTime(unixSeconds: number, options?: Intl.DateTimeFormatOptions): string;
export declare function shortDate(unixSeconds: number): string;
export declare function dayKey(unixSeconds: number): string;
export declare function isoOf(unixSeconds: unknown): string | null;
export declare function elapsedSince(sinceUnixSeconds: number, nowMs: number): number;
export declare function dayLabel(unixSeconds: number, now?: Date): string;
export declare function duration(seconds: number): string;
export declare function durationPrecise(seconds: number): string;
export declare function relative(unixSeconds: number, now?: number): string;
export declare function ago(unixSeconds: number, now?: number): string;
