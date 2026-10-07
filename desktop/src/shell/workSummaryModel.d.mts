/**
 * Types for `workSummaryModel.mjs`, which is plain JavaScript so `node --test`
 * can import it without a build step.
 */

import type { SessionRow } from "../types";

/** What waits on the person, what is open for review, what is being worked on — and the first conversation an agent is mid-turn in. */
export interface WorkSummary {
  waiting: number;
  review: number;
  working: number;
  busyScope: string | null;
}

/**
 * The counts from the shell's lists and the roster's tally. What waits is the
 * Inbox's count alone — every wait the roster knows is an Inbox row — so the
 * tally's `waiting` is not added again; its `working` is.
 */
export declare function workSummary(
  workspace: {
    inbox: readonly { needs_action?: readonly { gate_kind?: string | null }[] | null }[];
    waiting: number;
    working: Readonly<Record<string, readonly string[]>>;
  },
  sessions: { waiting: number; working: number },
  /** The roster's rows, when the caller holds them: `working` is then `sessionCountsModel.workingCount`'s, a turn counted once. */
  rows?: readonly SessionRow[] | null,
): WorkSummary;
