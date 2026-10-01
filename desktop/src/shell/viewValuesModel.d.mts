/** Types for `viewValuesModel.mjs`. */

export declare const MAX_TEXT: number;
export declare const MAX_WORDS: number;
export declare function textValue(raw: unknown): string | undefined;
export declare function flagValue(raw: unknown): boolean | undefined;
export declare function countValue(raw: unknown): number | undefined;
export declare function wordOf<W extends string>(offered: readonly W[]): (raw: unknown) => W | undefined;
export declare function idValue(raw: unknown): string | undefined;
export declare function wordsValue(raw: unknown): string[] | undefined;
export declare function wordsOf(set: Iterable<string>): string[];
export declare function sameKept(a: unknown, b: unknown): boolean;
