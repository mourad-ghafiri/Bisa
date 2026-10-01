export type Cleanup = "keep" | "ask" | "remove_when_merged";
export type AfterMerge = "ask" | "return_and_pull" | "stay";
export type StepId = "pull" | "delete" | "return";
export type StepOutcome = "done" | "skipped" | "failed" | "conflict";

export declare const CLEANUP: readonly Cleanup[];
export declare const AFTER_MERGE: readonly AfterMerge[];
export declare const STEP_ORDER: readonly StepId[];

export interface AfterMergeStep {
  id: StepId;
  label: string;
  detail: string;
  checked: boolean;
  available: boolean;
  reason: string | null;
}

export declare function afterMergePlan(input: {
  cleanup: string;
  afterMerge: string;
  pullMode: string;
  primaryBusy: number | null;
  defaultBranch: string | null;
  branch: string | null;
  dirty?: boolean;
  /** What still stands in the merged checkout — said on the delete step. */
  terminated?: TerminationCounts;
}): { mode: "none" | "silent" | "dialog"; steps: AfterMergeStep[] };

export declare function stepsToRun(steps: readonly AfterMergeStep[]): StepId[];
export declare function toggleStep(steps: readonly AfterMergeStep[], id: StepId): AfterMergeStep[];
export declare function summaryOf(outcomes: Partial<Record<StepId, StepOutcome>>, defaultBranch: string | null, terminated?: TerminationCounts): string;

import type { TerminationCounts } from "./closeWorkstreamModel.mjs";
