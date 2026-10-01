/** Types for `tabsFitModel.mjs`, plain JavaScript so `node --test` reads it. */

export interface FoldableTab {
  id: string;
  label: string;
  icon?: unknown;
  /** The turn in which the tab yields its word, lower first; none yields first, together. */
  fold?: number;
  count?: number | null;
}

export function foldGroups(tabs: readonly Pick<FoldableTab, "id" | "icon" | "fold">[]): string[][];
export function fitStage(needed: readonly (number | null | undefined)[], available: number | null | undefined, stages: number): number;
export function foldedIds(groups: readonly (readonly string[])[], stage: number): Set<string>;
export function stripKey(tabs: readonly FoldableTab[]): string;
export function glyphTitle(label: string, count?: number | null): string;
