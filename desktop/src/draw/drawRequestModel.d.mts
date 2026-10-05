/** Types for `drawRequestModel.mjs`. */
import type { AttachmentRef, DrawRequest, DrawResult } from "../types";

export declare const DRAW_PRESENCE_MS: number;
/** How many taken-up requests the bridge remembers. */
export declare const HANDLED_KEPT: number;
export declare const MERMAID_FONT_SIZE: number;
export declare const DRAWING_GONE: string;
export declare const DRAWING_MOVED: string;
export declare const NOT_A_SKELETON: string;
export declare const MERMAID_REFUSED: string;
export declare const SNAPSHOT_FAILED: string;
export declare const UNKNOWN_ACTION: string;

export type DrawPlan =
  | { kind: "refuse"; error: string }
  | { kind: "draw"; drawing: string; elements: unknown[]; replace: boolean }
  | { kind: "mermaid"; drawing: string; text: string; replace: boolean }
  | { kind: "snapshot"; drawing: string };

export declare function planDrawRequest(request: DrawRequest): DrawPlan;
export declare function mergeElements<E extends { id: string }>(current: readonly E[], incoming: readonly E[], replace: boolean): E[];
/** The hash a bridge save states: the open canvas's record when the shapes landed on it, else the hash the bridge read. */
export declare function saveBase(live: boolean, recordHash: string | null, readHash: string): string;
export declare function drawnResult(drawing: string, hash: string, elementCount: number): DrawResult;
export declare function snapshotResult(drawing: string, snapshot: AttachmentRef, size: { width: number; height: number }): DrawResult;
export declare function refusedResult(error: string, drawing?: string | null): DrawResult;
export declare function snapshotName(drawing: string, at?: Date): string;
export declare function drawingWords(count: number): string;
