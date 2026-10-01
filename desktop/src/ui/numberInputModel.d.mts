/**
 * Types for `numberInputModel.mjs`. This file is the only reason TypeScript
 * never has to read it.
 */

export declare function parseBounded(
  text: string | null | undefined,
  bounds?: { min?: number; max?: number; fallback?: number },
): number;
