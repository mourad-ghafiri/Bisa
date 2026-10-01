/** Types for `whereIWasModel.mjs`. */

/** One thing *Forget where I was* forgets. */
export type Forgotten = "memories" | "root" | "conversations" | "drafts";

export type ForgetOutcome = "busy" | "unsaved" | "forgotten";

export interface ForgetAsked {
  /** Also discard the messages being written. */
  drafts?: boolean;
}

export interface ForgetHands {
  /** Whether anything is unsaved. */
  dirty: () => boolean;
  /** Save it all; true when all of it saved. */
  save: () => Promise<boolean>;
  /** One hand a thing. */
  forget: Record<Forgotten, () => void>;
  /** Hear which thing stayed, and why; the rest are still forgotten. A seal that failed is heard as `seal`. */
  failed?: (what: Forgotten | "seal", error: unknown) => void;
  /** The memories take nothing more and write nothing more, until the window opens again. */
  seal?: () => void;
  /** Go home and open the window again. */
  leave: () => void | Promise<void>;
}

export interface ForgetFlow {
  run: (asked?: ForgetAsked) => Promise<ForgetOutcome>;
  running: () => boolean;
}

export declare const SEAL: "seal";
export declare const FORGOTTEN: readonly Forgotten[];
export declare const FORGOTTEN_ON_REQUEST: readonly Forgotten[];
export declare const WHERE_I_WAS_OUTCOMES: readonly ForgetOutcome[];
export declare function forgottenBy(asked?: ForgetAsked): Forgotten[];
export declare function forgetFlow(hands: ForgetHands): ForgetFlow;
