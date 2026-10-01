/** Types for `keyContextsModel.mjs`. */

export type WithinFact = "monaco" | "rendered" | "conflict" | "terminal" | "browser" | "files";

export interface KeyFacts {
  root: boolean;
  strip: boolean;
  designer: boolean;
  within: Partial<Record<WithinFact, boolean>>;
}

export declare const WITHIN: Readonly<Record<WithinFact, string>>;
export declare function scopesOf(facts: KeyFacts): string[];
