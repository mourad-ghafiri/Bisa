/** Types for `heldSignalsModel.mjs`. */
import type { EngineEvent, SignalView } from "../../types";

/** Who listens with a design's starts: a library workflow, or a goal. */
export interface ListeningHost {
  workflow?: string | null;
  goal?: string | null;
}
/** One signal held for a person to read. */
export interface HeldSignal {
  id: string;
  /** The start that heard it. */
  step: string;
  /** Where it came from, in words. */
  source: string;
  /** When it was written down, unix seconds. */
  at: number;
  /** The node's own reason, whole. */
  why: string;
}

export declare const HELD_PAGE: number;
export declare function hostOf(of: ListeningHost | null | undefined): string | null;
export declare function signalsQuery(host: string): { host: string; limit: number };
export declare function heldOf(signals: readonly SignalView[] | null | undefined, host: string | null | undefined): HeldSignal[];
export declare function heldWords(n: number): string | null;
export declare function movesSignalsOf(event: Pick<EngineEvent, "payload"> | null | undefined, host: string | null | undefined): boolean;
