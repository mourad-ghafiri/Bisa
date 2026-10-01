import type { GraphMatches, GraphRefScope, GraphRow, GraphWindow } from "../../types";

export const LANE_W: number;
export const NODE_R: number;

export interface Stroke {
  kind: "line" | "curve";
  /** The lane whose colour the stroke wears. */
  lane: number;
  x1: number;
  /** Row fraction, 0 at the top of the row, 1 at the bottom. */
  y1: number;
  x2: number;
  y2: number;
}

export function laneColor(lane: number): string;
export function laneX(lane: number): number;
export function strokesFor(row: GraphRow, prev: GraphRow | null): Stroke[];
export function lanesWidth(rows: GraphRow[]): number;
export function matches(row: GraphRow, query: string): boolean;
export const PAGE: number;
export type SparseRows = (GraphRow | undefined)[];
export function emptyRows(total: number): SparseRows;
export function mergeWindow(rows: SparseRows, w: GraphWindow): SparseRows;
export function holesIn(rows: SparseRows, first: number, last: number, page?: number): { from: number; count: number }[];
export function nextIndex(indices: readonly number[], cursor: number, dir: 1 | -1): number;
export function searchStatus(m: GraphMatches | null, cursor: number): string;
export function commitCount(w: { total: number; done: boolean; stale: boolean }): string;
export function layoutWord(w: { total: number; done: boolean; stale: boolean }): string | null;
export const REF_SCOPES: readonly GraphRefScope[];
export function refScopeWords(scope: GraphRefScope): { label: string; hint: string };
export interface HistoryMenuItem {
  id: string;
  label: string;
  icon: string | null;
  separatorBefore?: boolean;
  active?: boolean;
}
export function historyMenu(facts: { refs: GraphRefScope; searching: boolean }): HistoryMenuItem[];
export declare function currentBranchOf(headId: string | null, loaded: readonly { id: string; refs: readonly { kind: string; name: string }[] }[]): string | null;
export declare function cursorAfterKey(key: string, cursor: number, total: number): number | null;
