export type TagMatch = "any" | "all";
export interface TagFilterState {
  selected: readonly string[];
  match: TagMatch;
}
export declare const NO_TAG_FILTER: TagFilterState;
/** A tag filter read back from a screen's memory, or `undefined` for what is no filter. */
export declare function parseTagFilter(raw: unknown): TagFilterState | undefined;
export declare function passesTagFilter(tags: readonly string[] | null | undefined, filter: TagFilterState): boolean;
export declare function searchNeedle(query: string | null | undefined): string;
export declare function matchesWords(needle: string, fields: readonly (string | null | undefined)[]): boolean;
export declare function filterByTagsAndWords<T>(
  items: readonly T[],
  filter: TagFilterState,
  query: string | null | undefined,
  tagsOf: (item: T) => readonly string[] | null | undefined,
  fieldsOf: (item: T) => readonly (string | null | undefined)[],
): T[];
