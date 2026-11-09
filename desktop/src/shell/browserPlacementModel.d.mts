/** Types for `browserPlacementModel.mjs`, plain JavaScript so `node --test` reads it. */

import type { Rect } from "./centerSlotModel.mjs";

export interface HostSlot {
  rect: Rect | null;
  layer: string | null;
  key: string | null;
}
export interface PlacementFacts {
  center: HostSlot | null;
  aux: HostSlot | null;
  mayShow: boolean;
}
export interface SlotFacts {
  rect: { left: number; top: number; width: number; height: number } | null;
  layer: string | null;
  key: string | null;
}
export declare const HOSTS: readonly ("center" | "aux")[];
/** What a placement reads of a tab: whether it is kept out of sight, and the viewport it chose there, if any. */
export interface TabFacts {
  headless?: boolean;
  viewport?: { width: number; height: number } | null;
}
export declare function placementOf(key: string, facts: { center: SlotFacts | null; aux: SlotFacts | null; mayShow: boolean } & TabFacts): { host: "center" | "aux" | "offstage"; rect: { left: number; top: number; width: number; height: number } } | null;
export declare function isShown(key: string, facts: { center: SlotFacts | null; aux: SlotFacts | null; mayShow: boolean } & TabFacts): boolean;
export declare const OFFSTAGE_VIEWPORT: Readonly<{ width: number; height: number }>;
export declare const MIN_VIEWPORT: number;
export declare const MAX_VIEWPORT: number;
export declare const MAX_FULL_PAGE_HEIGHT: number;
export declare function offstageRect(viewport?: { width: number; height: number }): { left: number; top: number; width: number; height: number };
export declare function viewportOf(width: unknown, height: unknown): { width: number; height: number } | null;
export declare function fullPageHeight(pageHeight: unknown, viewport: { width: number; height: number }): number;
export declare function shownTab(slots: { center: { layer: string | null; key: string | null } | null; aux: { layer: string | null; key: string | null } | null }): string | null;
export declare const PLACEMENT_TOLERANCE: number;
export declare function placedAsAsked(asked: { left: number; top: number; width: number; height: number }, drawn: { left: number; top: number; width: number; height: number; shown: boolean }): boolean;
