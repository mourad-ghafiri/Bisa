/**
 * Types for `addonMediaModel.mjs`, which is plain JavaScript so `node --test`
 * imports it without a build step.
 */

import type { PlayerBox } from "./addonBridgeModel.mjs";

export type MediaStateWord = "ready" | "unstarted" | "playing" | "paused" | "buffering" | "ended" | "cued" | "error";
export const MEDIA_STATES: readonly MediaStateWord[];

/** The `media` event's payload: the player's state, its video's title, and — on an error — YouTube's code and the page's words. */
export interface MediaState {
  state: MediaStateWord;
  title: string;
  code?: string;
  words?: string;
}

export interface PageRect {
  left: number;
  top: number;
  width: number;
  height: number;
}

export function mediaKey(addonId: string): string;
export function playerPath(addonId: string, video: string): string;
export function pageRect(frame: PageRect, box: PlayerBox): PageRect | null;
export function playerShown(facts: { embedded: boolean; visible: boolean; shielded: boolean; mayShow: boolean; rect: PageRect | null }): boolean;
export function stateOf(message: unknown): MediaState | null;
