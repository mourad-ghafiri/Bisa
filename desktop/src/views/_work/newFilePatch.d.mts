/** Types for `newFilePatch.mjs`, plain JavaScript so `node --test` reads it. */

export declare function newFilePatch(path: string, text: string): string;
export declare function newFileWords(
  file: { binary?: boolean; size?: number; truncated?: boolean } | null | undefined,
): { sentence: string; binary: boolean; cut: boolean };
