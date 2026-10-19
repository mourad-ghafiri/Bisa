/**
 * Types for `addonSceneModel.mjs`, which is plain JavaScript so `node --test`
 * imports it without a build step.
 */

import type { ClearBox } from "../shell/browserClearModel.mjs";
import type { SceneMode } from "./addonBridgeModel.mjs";

export interface SceneViewport {
  width: number;
  height: number;
  top: number;
}
export interface SceneSlot {
  rect: { left: number; top: number; width: number; height: number } | null;
  visible: boolean;
  layer: string | null;
}
export interface SceneObstacle {
  x: number;
  y: number;
  width: number;
  height: number;
}
/** The `scene` event's payload. */
export interface SceneEventPayload {
  mode: SceneMode;
  viewport: { width: number; height: number };
  obstacles: SceneObstacle[];
  cuts: boolean;
}

export function sceneBox(viewport: SceneViewport): { left: number; top: number; width: number; height: number };
export function frameTaken(facts: { mode: string; hovering: boolean; shielded: boolean }): boolean;
export function pointInSolid(point: { x: number; y: number }, rects: ReadonlyArray<SceneObstacle>, top: number): boolean;
export function obstaclesOf(slots: ReadonlyArray<SceneSlot>, top: number): SceneObstacle[];
export function solidClears(rects: ReadonlyArray<SceneObstacle & { radius?: number }>, top: number): ClearBox[];
export function sceneEvent(facts: { mode: SceneMode; viewport: SceneViewport; slots: ReadonlyArray<SceneSlot>; cuts: boolean }): SceneEventPayload;
export function sceneKey(event: unknown): string;
export function modeAfter(current: SceneMode, asked: unknown): SceneMode;
