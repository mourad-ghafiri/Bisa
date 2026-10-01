import type { SearchHit, SearchSummary } from "../../types";

export declare function parseQuery(text: string): { needle: string; include: string[]; exclude: string[] };
export declare function nameResults(index: readonly string[], needle: string, cap?: number): string[];

export interface HitGroup {
  path: string;
  hits: { line: number; column: number; text: string }[];
  more: number;
}
export declare function groupHits(hits: readonly Pick<SearchHit, "path" | "line" | "column" | "text">[], perFile?: number): HitGroup[];
export declare function searchStatus(summary: SearchSummary | null, live: boolean, shown: number): string;

export type ResultRow =
  | { kind: "file"; key: string; path: string; count: number }
  | { kind: "hit"; key: string; path: string; line: number; column: number; text: string }
  | { kind: "more"; key: string; path: string; more: number };
export declare function resultRows(groups: readonly HitGroup[]): ResultRow[];
export declare function nextOpenable(rows: readonly ResultRow[], cursor: number, dir: 1 | -1): number;
export declare function splitHit(text: string, column: number, needleLength: number): [string, string, string];
