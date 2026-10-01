export declare const SECTION_CAPS: Readonly<Record<string, number>>;
export declare const MAX_ROWS: number;
export declare function scorePath(queryLower: string, path: string): number | null;
export declare function rankPaths(query: string, paths: readonly string[], limit?: number): { path: string; score: number }[];
export declare function parsePrefix(raw: string): { prefix: ">" | ":" | "#" | "@" | null; query: string };
/** Whether the index lists a path at all: not what the ignore rules match, not a hidden name. */
export declare function listed(path: string, ignored?: boolean | null): boolean;
export declare function patchIndex(
  paths: readonly string[],
  event: { kind: string; path: string; from?: string | null; dir?: boolean; ignored?: boolean },
): readonly string[] | null;
export declare const MAX_ITEMS: number;
export declare function paletteNeedle(q: string, mode: "all" | "places" | "commands" | "line"): string;
export declare function itemMatches(item: { label: string; hint?: string | null; keywords?: string | null }, needle: string): boolean;
export declare function admit(counts: Map<string, number>, group: string): boolean;
export declare function groupItems<T extends { group: string }>(items: readonly T[]): { groups: [string, T[]][]; flat: T[] };
