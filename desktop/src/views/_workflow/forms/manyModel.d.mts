/** Types for `manyModel.mjs`. */

/** A field that takes one value or a list, as the wire carries it. */
export type OneOrMany<T> = T | T[];

export declare function fromList<T>(list: readonly T[] | null | undefined): OneOrMany<T> | null;
export declare function hasWord<T>(v: OneOrMany<T> | null | undefined, word: T): boolean;
export declare function toggleWord<T>(v: OneOrMany<T> | null | undefined, word: T): OneOrMany<T> | null;
export declare function addWord<T>(v: OneOrMany<T> | null | undefined, word: T): OneOrMany<T> | null;
export declare function removeWord<T>(v: OneOrMany<T> | null | undefined, word: T): OneOrMany<T> | null;
export declare function wordsOr<T>(v: OneOrMany<T> | null | undefined, fallback: T | null): T[];
export declare function splitWords<T, W extends string>(v: OneOrMany<T | W> | null | undefined, words: readonly W[]): { words: W[]; rest: T[] };
export declare function joinWords<A, B>(words: OneOrMany<A> | null | undefined, rest: OneOrMany<B> | null | undefined): OneOrMany<A | B> | null;
