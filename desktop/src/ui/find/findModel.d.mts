/**
 * Types for `findModel.mjs`, which is plain JavaScript so `node --test`
 * can import it without a build step.
 */

/** What a find bar holds. */
export interface Find {
  query: string;
  regex: boolean;
  caseSensitive: boolean;
  replacement: string;
}

export interface Match {
  start: number;
  end: number;
}

export declare function emptyFind(): Find;
export declare function hasQuery(find: Pick<Find, "query"> | null | undefined): boolean;
export declare function compileFind(find: Pick<Find, "query" | "regex" | "caseSensitive">): RegExp | null;
export declare function matchesOf(text: string, find: Pick<Find, "query" | "regex" | "caseSensitive">): Match[];
export declare function replaceAll(text: string, find: Find): { text: string; count: number };
export declare function replaceOne(text: string, find: Find, index: number): { text: string; replaced: boolean };
export declare function stepIndex(index: number, count: number, dir: "next" | "previous"): number;
export declare function countWords(index: number, count: number | null | undefined): string;
