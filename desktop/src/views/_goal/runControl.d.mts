/**
 * Types for `runControl.mjs`. This file is the only reason TypeScript never
 * has to read it.
 */

import type { CancelCause, Goal, GuidanceInfo, RunStatus, RunSummary, WorkflowRun } from "../../types";

export interface StartVerb {
  /** *Start run…*, *Adopt and start…*, *New run…*, *Start listening…*, *Run now…*. */
  label: string;
  /** True when the run would queue behind a live one. */
  queues: boolean;
  /** True when the start adopts the Workflow Agent's proposal. */
  adopt: boolean;
  /** The design begins on events: the start arms them — the goal listens — rather than running. */
  listen?: boolean;
  /** A listening goal's run by hand: at this start step, its design's start by hand. */
  at?: string;
}
export interface ListenVerb {
  id: "stop" | "again";
  label: string;
}
export interface StopVerb {
  label: "Stop";
  live: boolean;
  queued: number;
  /** The sessions working on the goal a stop ends — offered on them alone when no run is going. */
  sessions: number;
}
export interface RestartVerb {
  label: "Restart";
}
export interface RunVerbs {
  start: StartVerb | null;
  stop: StopVerb | null;
  restart: RestartVerb | null;
}
export interface RunWords {
  word: string;
  tone: "quiet" | "accent" | "warn" | "ok" | "danger";
  at: number | null;
  atWord: string;
}
export interface RunRow {
  id: string;
  index: number;
  status: RunStatus;
  revision: number;
  workflow: string;
  words: RunWords;
  withdraw: boolean;
  /** Still to end: going, or queued behind the run that is. */
  live: boolean;
}

export declare function runIsLive(run: WorkflowRun | null | undefined): boolean;
export declare function runIsQueued(run: WorkflowRun | null | undefined): boolean;
export declare function summaryIsLive(summary: RunSummary | null | undefined): boolean;
export declare function summaryIsQueued(summary: RunSummary | null | undefined): boolean;
export declare function anyRunLive(run: WorkflowRun | null | undefined, runs: readonly RunSummary[] | null | undefined): boolean;
export declare function panelFrozen(run: WorkflowRun | null | undefined, runs: readonly RunSummary[] | null | undefined): boolean;
export declare const FROZEN_HINT: string;
export declare function liveRun(run: WorkflowRun | null | undefined, runs: readonly RunSummary[] | null | undefined): string | null;
export declare function queuedRuns(runs: readonly RunSummary[] | null | undefined): RunSummary[];
export declare function runIndex(runs: readonly RunSummary[] | null | undefined, id: string): number | null;
export declare function runVerbs(args: {
  goal: Goal;
  run: WorkflowRun | null | undefined;
  runs: readonly RunSummary[] | null | undefined;
  guidance: GuidanceInfo | null | undefined;
  proposed: boolean;
  startable: boolean;
  /** The design begins on events. */
  listens?: boolean;
  /** The design's start by hand, when it has one. */
  manualEntry?: string | null;
  /** The sessions working on the goal (`liveSessionsOf`): a stop is offered on them too. */
  liveSessions?: number;
}): RunVerbs;
/** The sessions working on a goal that a stop would end: running, or waiting on the person. */
export declare function liveSessionsOf(rows: readonly { goal?: string | null; state: unknown }[] | null | undefined, goalId: string): number;
/** `liveSessionsOf` for every goal at once. */
export declare function sessionsByGoal(rows: readonly { goal?: string | null; state: unknown }[] | null | undefined): Map<string, number>;
/** The open goals spawned by a goal, recursively, from the rows the window holds. */
export declare function spawnedOpen(goals: readonly { id: string; status?: string; closed?: unknown; origin?: { origin?: string; parent?: string } | null }[] | null | undefined, id: string): number;
export declare function listenVerb(goal: Goal | null | undefined): ListenVerb | null;
export declare function cancelWords(cause: CancelCause | null | undefined): string;
/** A run's status read off the run itself — the core's `WorkflowRun::status`; `null` for no run. */
export declare function runStatus(run: Pick<WorkflowRun, "started_at" | "outcome" | "cancelled" | "steps"> | null | undefined): RunStatus | null;
export declare function runWords(summary: RunSummary | null | undefined): RunWords;
export declare function stopWords(args: { live: boolean; queued: number; liveSteps?: number   /** The sessions working on the goal, said when no live run says it already. */
  sessions?: number;
  /** The open goals it spawned, stopped with it. */
  children?: number;
}): string;
export declare function restartWords(args: { live: boolean; queued: number   /** The open goals it spawned, stopped with it. */
  children?: number;
}): string | null;
export declare function runRows(runs: readonly RunSummary[] | null | undefined): RunRow[];
