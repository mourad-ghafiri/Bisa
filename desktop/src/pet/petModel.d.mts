/**
 * Types for `petModel.mjs`. This file is the only reason TypeScript never has
 * to read it.
 */

import type { Route } from "../router";
import type { SessionState } from "../types";

export type PetState =
  | "idle"
  | "running-right"
  | "running-left"
  | "waving"
  | "jumping"
  | "failed"
  | "waiting"
  | "running"
  | "review";

export interface SheetGeometry {
  width: number;
  height: number;
  cols: number;
  rows: number;
  cellWidth: number;
  cellHeight: number;
}

export declare const STATES: readonly PetState[];
export declare const SHEET: SheetGeometry;
export declare const FRAME_MS: number;
export declare const IDLE_SLOWDOWN: number;

/** What is true right now, as counts. */
export interface Activity {
  waiting?: number;
  review?: number;
  working?: number;
}

export declare function standingState(activity?: Activity): PetState;
/** A roster session's state, in the pet's rows — the standing state while following. */
export declare function petStateOfSession(state: SessionState | string): PetState;
/** The once-through state a followed session's edge earns. */
export declare function transientForTransition(prev: SessionState | string | null, next: SessionState | string): PetState | null;
export declare function transientFor(payload: string | { type: string; [k: string]: unknown }): PetState | null;

/** The harness session the pet follows, when it does: the workstream it stands in. */
export interface Follow {
  workstream: string;
}
export declare function dragState(dx: number, threshold?: number): PetState | null;

/** Whether a cell holds any non-transparent pixel. */
export type AlphaProbe = (row: number, col: number) => boolean;

export declare function frameCounts(alphaAt: AlphaProbe, sheet?: SheetGeometry): number[];
export declare function alphaProbe(
  data: Uint8ClampedArray | number[],
  sheet?: SheetGeometry,
  step?: number,
): AlphaProbe;

/** One cell as it is drawn, plus the sheet size behind it. */
export interface SpriteBox {
  width: number;
  height: number;
  sheetWidth: number;
  sheetHeight: number;
}

export declare const PET_HEIGHT: number;

export declare const PET_SIZE_MIN: number;
export declare const PET_SIZE_MAX: number;
export declare const PET_SIZE_STEP: number;
export declare const PET_SIZE_DEFAULT: number;

/** A stored or typed size percentage, made safe to render at. */
export declare function clampPetSize(size: unknown): number;

/** The height in pixels a size percentage asks for. */
export declare function petHeight(size: unknown): number;

export declare function spriteBox(
  naturalWidth: number,
  naturalHeight: number,
  targetHeight?: number,
  sheet?: SheetGeometry,
): SpriteBox;

export declare function cellOffset(
  row: number,
  frame: number,
  box: SpriteBox,
): { x: number; y: number };

export declare function frameDuration(state: PetState): number;

/** How a state plays: the manifest's frames and durations, else the probed count at the fixed pace. */
export interface FramePlan {
  frames: number;
  durations: number[];
  fromManifest: boolean;
}
export declare function framePlan(def: import("../types").PetDef | null | undefined, state: PetState, probed: number): FramePlan;

export declare const DEFAULT_PET_ID: "bisa-pets.midnight-shipping.moonrice";
export declare function defaultPet(pets: readonly { id: string; origin?: string }[], last: string | null | undefined): string | null;
/** What the pet is doing, in the catalog's words. */
export declare function stateWords(state: PetState): string;
export declare function originWords(origin: string | null | undefined): "built in" | "yours";
export declare function tileWords(def: import("../types").PetDef | null | undefined): { tagline: string; chips: string[] };
export declare function routeFor(state: PetState, scope?: string | null, follow?: Follow | null): Route | null;
export declare function rowOf(state: PetState): number;
