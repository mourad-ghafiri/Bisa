/**
 * Types for `workSummaryModel.mjs`, which is plain JavaScript so `node --test`
 * can import it without a build step.
 */

/** What waits on the person, what is open for review, what is being worked on — and the first conversation an agent is mid-turn in. */
export interface WorkSummary {
  waiting: number;
  review: number;
  working: number;
  busyScope: string | null;
}

export declare function workSummary(
  workspace: {
    inbox: readonly { needs_action?: readonly { gate_kind?: string | null }[] | null }[];
    waiting: number;
    working: Readonly<Record<string, readonly string[]>>;
  },
  sessions: { waiting: number; working: number },
): WorkSummary;
