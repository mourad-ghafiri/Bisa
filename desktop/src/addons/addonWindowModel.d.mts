/** Types for `addonWindowModel.mjs`. */
import type { Placement } from "../ui/dockModel.mjs";

export const BAR_HEIGHT: number;
export const DEFAULT_MIN: { readonly width: number; readonly height: number };
export const EDGE: number;
export const STAGGER: number;
export const BOTTOM_RIGHT_CLEARANCE: number;
export const VIEWPORT_MARGIN: number;

export interface Size {
  width: number;
  height: number;
}
export interface SizeBounds {
  minWidth: number;
  minHeight: number;
  maxWidth: number;
  maxHeight: number;
}
export interface ManifestWindowLike {
  width: number;
  height: number;
  min_width?: number | null;
  min_height?: number | null;
  max_width?: number | null;
  max_height?: number | null;
}
export type AddonDockCorner = "top_left" | "top_right" | "bottom_left" | "bottom_right";
export interface SlotLike {
  visible: boolean;
  layer: string | null;
  rect: { left: number; top: number; width: number; height: number } | null;
}
export interface WindowPref {
  dock: Placement;
  size: Size;
}

export function sizeBounds(manifestWindow: ManifestWindowLike, viewport: { width: number; height: number; top: number }): SizeBounds;
export function clampSize(size: Size, bounds: SizeBounds): Size;
export function sizeFrom(start: Size, dx: number, dy: number, bounds: SizeBounds): Size;
export function placementAfterResize(placement: Placement, before: Size, after: Size): Placement;
export function defaultPlacement(dock: AddonDockCorner | string | undefined, index?: number): Placement;
export function hiddenByLayer(box: { left: number; top: number }, size: Size, slots: readonly SlotLike[], cutsAround?: boolean): boolean;
export function windowPrefFrom(raw: unknown, opening: Size, fallbackDock: Placement, bounds: SizeBounds): WindowPref;
