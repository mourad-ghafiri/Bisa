/**
 * Types for `runView.mjs`. This file is the only reason TypeScript never has
 * to read it.
 */

import type { RunStatus, RunSummary, Step, StepRecord, StepState, WorkflowRun } from "../../types";
import type { GraphEdge } from "./workflowGraph.mjs";

export type Role = "text-dim" | "accent" | "warn" | "ok" | "danger";
export type StepAction = "answer" | "done" | "decide" | "release" | "open";
export type EdgeTone = "default" | "taken" | "skipped";

export declare const STEP_TONE_TOKENS: Record<StepState["state"], Role>;
/** The states the legend above a run's canvas names, in order. */
export declare const LEGEND_STATES: readonly StepState["state"][];
/** The word for a step's state. */
export declare function stepStateWord(state: string): string;
export declare function stepTone(run: WorkflowRun | null | undefined, id: string): Role;
export declare function branchesChosen(state: StepState | null | undefined): string[];
export declare function stepLabel(run: WorkflowRun | null | undefined, id: string): string;
export declare function progress(run: WorkflowRun | null | undefined): number;
export declare function currentSteps(run: WorkflowRun | null | undefined): string[];
export declare function firedBoundaries(run: WorkflowRun | null | undefined, id: string): Set<string>;
export declare function stepActions(step: Step, record: StepRecord | null | undefined): StepAction[];
export declare function edgeTone(run: WorkflowRun | null | undefined, edge: GraphEdge): EdgeTone;
export declare function failedStep(run: WorkflowRun | null | undefined): { id: string; name: string; error: string | null } | null;

/** A moment the strip names: the word before it, and when. */
export interface OverlayMoment {
  word: string;
  at: number | null;
}
/** The strip above a run's canvas, as facts. */
export interface OverlayFacts {
  /** The run's status read off the run itself — the core's projection. */
  status: RunStatus;
  /** The status in words: its place when queued, its cause when cancelled. */
  word: string;
  tone: "quiet" | "accent" | "warn" | "ok" | "danger";
  queued: boolean;
  /** Its number among the goal's runs; `null` when the list does not hold it. */
  index: number | null;
  revision: number | null;
  /** How far it got, 0 to 100. */
  percent: number;
  /** The steps live right now. */
  live: string[];
  /** When it began: started, or queued while it waits its turn. */
  began: OverlayMoment;
  /** How it ended: finished, or the cause of its cancel; `null` while it goes. */
  ended: OverlayMoment | null;
}
export declare function overlayFacts(run: WorkflowRun, runs: readonly RunSummary[] | null | undefined): OverlayFacts;
