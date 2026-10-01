/** Types for `boundaryModel.mjs`. */
import type { Boundary, BoundaryOn, Step } from "../../../types";

export type BoundaryEvent = BoundaryOn["event"];
export type BoundaryActName = "divert" | "notify" | "emit";

export interface BoundaryChip {
  name: string;
  event: BoundaryEvent;
  act: BoundaryActName;
  /** It stops the step and takes its own path: a handle, a solid border. */
  diverts: boolean;
  words: string;
}

export type RenameResult = { ok: true; step: Step } | { ok: false; reason: string };

export declare const DEFAULT_REMINDERS: number;
export declare const DAY_SECS: number;
export declare const OUT_WITH_DIVERTS: number;
export declare function mayCarryBoundaries(step: Step | null | undefined): boolean;
export declare function actsFor(event: BoundaryEvent | string | undefined): BoundaryActName[];
export declare function freshBoundaryName(step: Step | null | undefined, root: string): string;
export declare function blankOn(event: BoundaryEvent): BoundaryOn;
export declare function blankAct(act: BoundaryActName): Record<string, unknown> & { act: BoundaryActName };
export declare function blankBoundary(step: Step, event: BoundaryEvent): Boundary;
export declare function boundaryOf(step: Step | null | undefined, name: string): Boundary | null;
export declare function addBoundary<S extends Step>(step: S, event: BoundaryEvent): S;
export declare function replaceBoundary<S extends Step>(step: S, name: string, next: Boundary): S;
export declare function setEvent<S extends Step>(step: S, name: string, event: BoundaryEvent): S;
export declare function setAct<S extends Step>(step: S, name: string, act: BoundaryActName): S;
export declare function renameBoundaryOn(step: Step, from: string, to: string): RenameResult;
export declare function removeBoundaryOn<S extends Step>(step: S, name: string): S;
export declare function divertOffsets(step: Step | null | undefined): Map<string, number>;
export declare function ownOffsets(step: Step | null | undefined): { out: number | null; branches: Map<string, number> };
export declare function chipOf(boundary: Boundary): BoundaryChip;
export declare function consequence(step: Step, boundary: Boundary): string;
