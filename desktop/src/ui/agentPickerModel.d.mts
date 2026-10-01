/**
 * Types for `agentPickerModel.mjs`, which is plain JavaScript so `node --test`
 * can import it without a build step. This file is the only reason TypeScript
 * never has to read it.
 *
 * The shape is structural rather than an import of `AgentCandidate`: the model
 * is about ids, names and a few strings to search, and the component's richer
 * type widens to it.
 */

export interface PickerCandidate {
  id: string;
  name: string;
  description?: string | null;
  harness?: string | null;
  tags?: string[];
  /** Header this row sits under. Absent means one unlabelled bucket. */
  group?: string;
}

export type CursorAction =
  | { type: "move"; index: number }
  | { type: "pick"; index: number }
  | { type: "dismiss" };

export declare function haystack(c: PickerCandidate): string;
export declare function matches(c: PickerCandidate, query: string): boolean;
export declare function rank<T extends PickerCandidate>(
  candidates: readonly T[] | null | undefined,
  query: string,
  limit?: number,
): T[];
export declare function groupRows<T extends PickerCandidate>(
  candidates: readonly T[] | null | undefined,
): { label: string; rows: T[] }[];
export declare function clampCursor(cursor: number | null | undefined, count: number): number;
export declare function cursorAction(
  count: number,
  cursor: number | null | undefined,
  key: string,
): CursorAction | null;
export declare function toggleValue(
  value: readonly string[] | null | undefined,
  id: string,
  max?: number,
): string[];
