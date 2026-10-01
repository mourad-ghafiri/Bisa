/** Types for `workflowVerbs.mjs`. */
import type { Reference, WorkflowRow } from "../../types";

export interface RunWorkflowVerb {
  /** *Run…*, or *Test run…* for a workflow only events begin. */
  label: string;
  /** Only events begin it: the run is a test run, as if one of its events happened. */
  test: boolean;
}
export interface EveryVerb {
  label: string;
  /** How many of its runs of the workspace are going. */
  live: number;
}
export interface TurnVerb {
  label: string;
}
export interface WorkflowVerbs {
  run: RunWorkflowVerb | null;
  stop: EveryVerb | null;
  restart: EveryVerb | null;
  turnOn: TurnVerb | null;
  turnOff: TurnVerb | null;
}
export declare function liveGoals(usedBy: readonly Reference[] | null | undefined): Reference[];
export declare function workflowVerbs(row: WorkflowRow | null | undefined): WorkflowVerbs;
export declare function stopEveryWords(live: number): string;
export declare function restartEveryWords(live: number): string;
