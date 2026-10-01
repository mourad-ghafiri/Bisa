/** Types for `browserShotModel.mjs`, plain JavaScript so `node --test` reads it. */

export declare const SHOT_WIDTH_KEY: string;
export declare const MIN_SHOT_WIDTH: number;
export declare const MAX_SHOT_WIDTH: number;
export declare const DEFAULT_SHOT_WIDTH: number;
export declare function shotWidth(value: unknown): number;
export declare function shotName(key: string, at?: Date): string;
export declare function pngSize(bytes: Uint8Array): { width: number; height: number } | null;
export declare function shotWords(size: { width: number; height: number }): string;
export declare const NOT_SHOWN_WORDS: string;
export declare const CLIPBOARD_REFUSED: string;
