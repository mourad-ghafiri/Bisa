/**
 * Types for `workflowLayout.mjs`. This file is the only reason TypeScript
 * never has to read it.
 */

import type { Definition } from "./workflowGraph.mjs";

export declare const GRID: number;
export declare const NODE_WIDTH: number;
export declare const NODE_HEIGHT: number;
export declare const BOUNDARY_ROW: number;
export declare const GAP_Y: number;

export interface Point {
  x: number;
  y: number;
}

export interface Size {
  width: number;
  height: number;
}

export interface Box extends Point, Size {}

export interface Layout {
  /** Every step's top-left corner: its own position, or the derived one for a step that has none. */
  positions: Map<string, Point>;
  /** The edge ids the run machine treats as loops — drawn around the side, not ranked. */
  loops: Set<string>;
  size: Size;
}

export declare function estimateOf(step: { boundaries?: readonly unknown[] | null } | null | undefined): Size;
export declare function sideOnly(edges: readonly { kind: string; to: string; loop: boolean }[]): Set<string>;
export declare function snapTo(point: Point, grid?: number): Point;
export declare function overlaps(a: Box, b: Box): boolean;
export declare function nudgeFree(positions: Map<string, Point>, sizes: Map<string, Size> | null | undefined, id: string, point: Point, grid?: number): Point;
export declare function layout(wf: Definition): Layout;
export declare function withPositions<D extends Definition>(wf: D, positions: Map<string, Point>): D;
export declare function tidy<D extends Definition>(wf: D, options?: { sizes?: Map<string, Size> }): D;
