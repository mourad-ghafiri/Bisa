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
export declare function placementOf(key: string, facts: { center: SlotFacts | null; aux: SlotFacts | null; mayShow: boolean; headless?: boolean }): { host: "center" | "aux" | "offstage"; rect: { left: number; top: number; width: number; height: number } } | null;
export declare function isShown(key: string, facts: { center: SlotFacts | null; aux: SlotFacts | null; mayShow: boolean; headless?: boolean }): boolean;
export declare const OFFSTAGE_VIEWPORT: Readonly<{ width: number; height: number }>;
export declare function offstageRect(): { left: number; top: number; width: number; height: number };
export declare function shownTab(slots: { center: { layer: string | null; key: string | null } | null; aux: { layer: string | null; key: string | null } | null }): string | null;
export declare const PLACEMENT_TOLERANCE: number;
export declare function placedAsAsked(asked: { left: number; top: number; width: number; height: number }, drawn: { left: number; top: number; width: number; height: number; shown: boolean }): boolean;
