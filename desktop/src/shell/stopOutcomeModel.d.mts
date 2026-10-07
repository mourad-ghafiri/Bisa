/**
 * Types for `stopOutcomeModel.mjs`, which is plain JavaScript so `node --test`
 * can import it without a build step.
 */

/** The node's `ended` block, as the desktop reads it. */
export interface StopOutcomeBlock {
  sessions: number;
  terminated: number;
  still_live: number;
  children: readonly string[];
}
export interface EndedOpts {
  closed?: boolean;
  sessionsSaid?: boolean;
}
export declare const NOTHING_ENDED: StopOutcomeBlock;
export declare function endedOf(answer: unknown): StopOutcomeBlock | null;
export declare function endedParts(ended: StopOutcomeBlock | null | undefined, opts?: EndedOpts): string[];
export declare function endedWords(ended: StopOutcomeBlock | null | undefined, opts?: EndedOpts): string | null;
export declare function saidAfter(lead: string, ended: StopOutcomeBlock | null | undefined, opts?: EndedOpts): string;
export declare function stopTone(ended: StopOutcomeBlock | null | undefined): "info" | "ok";
export declare function willStopWords(facts?: { sessions?: number; children?: number; closing?: boolean }): string | null;
