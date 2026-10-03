/**
 * Types for `sidebarModel.mjs`, which is plain JavaScript so `node --test` can
 * import it without a build step. This file is the only reason TypeScript
 * never has to read it.
 */

import type { InboxRow } from "../types";
import type { Route } from "../router";

export type SidebarMode = "expanded" | "collapsed";
/** Two counts, never one sum: what needs the person (the accent's) and what is merely unread (neutral). */
export interface InboxBadge {
  needs: number;
  unread: number;
  needsTitle: string | null;
  unreadTitle: string | null;
  /** Both counts' words, for a tooltip that names the whole. */
  title: string | null;
}
export interface RailBadge {
  count: number;
  tone: "accent" | "neutral";
  title: string | null;
}
export interface RailDoor {
  key: string;
  label: string;
  route: Route;
  badge: RailBadge | null;
  /** A neutral dot beside an accent count: something is unread besides what is owed. */
  unreadDot: boolean;
  /** The count whose rise replays the arrival. */
  arrival: number;
  live: string | null;
  note: string | null;
}

export declare const SIDEBAR_MODES: readonly SidebarMode[];
export declare const SIDEBAR_MODE_KEY: string;
export declare const RAIL_WIDTH: number;
export declare const RAIL_DOOR: number;
export declare const RAIL_OVERHANG: number;
export declare function railGutter(): number;
export declare function readMode(stored: unknown): SidebarMode;
export declare function toggledMode(mode: SidebarMode): SidebarMode;
export declare function toggleWords(mode: SidebarMode): string;
export declare function needsWords(needs: number): string;
export declare function inboxBadge(rows: readonly InboxRow[] | null | undefined): InboxBadge;
export declare function railDoors(ws: {
  inbox?: readonly InboxRow[];
  channels?: readonly { channel: { id: string }; unread_count?: number }[];
  dms?: readonly { channel: { id: string }; unread_count?: number }[];
  hosted?: readonly object[];
  unread?: Readonly<Record<string, number>>;
  working?: Readonly<Record<string, readonly string[]>>;
}, nav: readonly { key: string; label: string; route: Route }[]): RailDoor[];
export declare function railTooltip(door: Pick<RailDoor, "label" | "badge" | "live" | "note">): string;
