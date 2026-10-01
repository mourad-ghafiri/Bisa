/** Types for `checkpointModel.mjs`. */

export declare const FAILURES_SAID: number;
export interface FailureTally {
  failed: (key: string) => boolean;
  succeeded: (key: string) => void;
  forget: (key: string) => void;
  count: (key: string) => number;
}
export declare function failureTally(): FailureTally;
