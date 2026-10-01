/**
 * Types for `dockModel.mjs`. This file is the only reason TypeScript never has
 * to read it.
 */

export type HorizontalEdge = "left" | "right";
export type VerticalEdge = "top" | "bottom";

/** Where a dock sits: px from the nearer edge on each axis. The stored fact. */
export interface Placement {
  h: HorizontalEdge;
  /** From the `h` edge to the dock's near side. */
  x: number;
  v: VerticalEdge;
  /** From the `v` edge to the dock's near side. */
  y: number;
}

/** Where a dock is painted: px from the viewport's top-left. Never stored. */
export interface Box {
  left: number;
  top: number;
}

export interface Viewport {
  width: number;
  height: number;
  /** The dock's own edge length, for a square dock. Default 40. */
  size?: number;
  /** Width, when the dock is not square. Defaults to `size`. */
  sizeX?: number;
  /** Height, when the dock is not square. Defaults to `size`. */
  sizeY?: number;
  /** The chrome's height: the dock never paints over it. Required — the number is the theme's (`--spacing-chrome`), not this model's. */
  top: number;
  /** Gap kept from every other edge. Default 8. */
  margin?: number;
}

export declare function dockBox(placement: Placement, viewport: Viewport): Box;
export declare function placementOf(box: Box, viewport: Viewport): Placement;
export declare function placementFrom<T>(value: unknown, fallback: T): Placement | T;
export declare function samePlacement(a: Placement, b: Placement): boolean;
