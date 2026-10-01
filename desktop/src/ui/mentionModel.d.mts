/**
 * Types for `mentionModel.mjs`, which is plain JavaScript so `node --test` can
 * import it without a build step. This file is the only reason TypeScript
 * never has to read it.
 *
 * The shape is structural rather than an import of `Mentionable`: the model is
 * about ids, names and one flag, and the component's richer type widens to it.
 */

export interface MentionCandidate {
  id: string;
  name: string;
  /** Offer this entry in the `@`-picker. Absent means yes. */
  suggest?: boolean;
}

/** Where an open `@`-run sits in the body, and what has been typed into it. */
export interface MentionSpan {
  /** Index of the `@`. */
  at: number;
  /** Index just past the query — the caret. */
  end: number;
  query: string;
}

export declare const SUGGESTION_LIMIT: number;

export declare function escapeRe(s: string): string;
export declare function isSuggestable(m: MentionCandidate): boolean;
export declare function suggestions<T extends MentionCandidate>(
  mentionables: readonly T[] | null | undefined,
  query: string,
  limit?: number,
): T[];
export declare function mentionsIn<T extends MentionCandidate>(
  mentionables: readonly T[] | null | undefined,
  text: string,
): string[];
export declare function mentionQuery<T extends MentionCandidate>(
  text: string,
  caret: number,
  mentionables: readonly T[] | null | undefined,
): MentionSpan | null;
export declare function insertMention(
  text: string,
  span: MentionSpan,
  name: string,
): { text: string; caret: number };
