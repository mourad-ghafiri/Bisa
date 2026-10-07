/** Types for `closeFlowModel.mjs`. */

export type CloseOutcome = "busy" | "cancelled" | "unsaved" | "closed";

export interface CloseHands {
  /** The quit switch (`desktop.confirm_quit`). */
  confirms: () => boolean;
  /** The question, answered yes or no. */
  ask: () => Promise<boolean>;
  /** Whether anything is unsaved. */
  dirty: () => boolean;
  /** Save it all; true when all of it saved. */
  save: () => Promise<boolean>;
  /** Say that the window stays: a save failed. */
  unsaved: () => void;
  /** Write what is remembered — where the person was, how each screen stood — and wait for it to settle. */
  keep: () => Promise<void>;
  /** Hear why a keep failed; the way out goes on. */
  keepFailed?: (error: unknown) => void;
  /** The tree that would ask and save is gone (the root crashed): neither is tried. */
  bare?: () => boolean;
}

export interface CloseFlow {
  run: (finish: () => Promise<void>) => Promise<CloseOutcome>;
  running: () => boolean;
}

export declare const CLOSE_OUTCOMES: readonly CloseOutcome[];
export declare function declines(outcome: CloseOutcome | string): boolean;
export declare function saveEvery<T>(items: readonly T[], saveOne: (item: T) => Promise<boolean>, onError?: (item: T, error: unknown) => void): Promise<boolean>;
export declare function closeFlow(hands: CloseHands): CloseFlow;
