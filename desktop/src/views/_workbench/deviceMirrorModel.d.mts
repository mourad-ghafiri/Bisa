/** Types for `deviceMirrorModel.mjs`, plain JavaScript so `node --test` reads it (ide/19). */

export declare const FRAME_MS: number;
export declare const ERROR_MS: number;
export declare const MAX_ERRORS: number;

export interface MirrorBox {
  clientWidth: number;
  clientHeight: number;
  naturalWidth: number;
  naturalHeight: number;
}
export interface Mark {
  x: number;
  y: number;
  width: number;
  height: number;
}
export interface Cadence {
  poll: boolean;
  every: number;
  reason: string | null;
  /** The device stopped answering — the one pause a person ends by asking again. */
  stopped: boolean;
}

export declare function mirrorCadence(facts: { up: boolean; awake: boolean; focused: boolean; errors: number }): Cadence;
export declare function drawnRect(box: MirrorBox): { left: number; top: number; width: number; height: number; scale: number } | null;
export declare function markFromDrag(start: { x: number; y: number }, end: { x: number; y: number }, box: MirrorBox): Mark | null;
export declare function markCss(mark: Mark, box: MirrorBox): { left: number; top: number; width: number; height: number } | null;
export declare function bootDoorWords(device: { name: string; kind: "simulator" | "emulator" | "physical"; state: string }): { title: string; hint: string; verb: string | null };
