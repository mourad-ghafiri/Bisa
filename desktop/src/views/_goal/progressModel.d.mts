import type { Answer, RunStrip, StepState, WorkflowRun } from "../../types";

export type StepHolder = "you" | "agents" | "world" | null;

export interface ProgressRow {
  id: string;
  name: string;
  kind: string;
  state: StepState;
  holder: StepHolder;
  startedAt: number | null;
  finishedAt: number | null;
  durationSecs: number | null;
  /** How long it took, or has been going, in words; `null` for a step that never started. */
  duration: string | null;
  workItem: string | null;
  output: unknown | null;
  error: string | null;
  answer: Answer | null;
  visits: number;
  actions: string[];
  current: boolean;
}

export declare function stepHolder(kind: string, state: string, until?: string | null): StepHolder;
/** The rows: a run's frozen steps, or the strip's ghosted plan. Of the strip it reads the live steps and the plan, nothing else. */
export declare function progressRows(run: WorkflowRun | null | undefined, strip: Pick<RunStrip, "current" | "steps">, now?: number): ProgressRow[];
/** Whether a step's row stands open: what a person said of it by hand, else open while it is live. */
export declare function rowOpen(said: ReadonlySet<string>, id: string, live: boolean): boolean;
/** What a person has said after pressing a step's row. */
export declare function rowPressed(said: ReadonlySet<string>, id: string, live: boolean): ReadonlySet<string>;
export declare function progressCount(strip: RunStrip | null | undefined): { reached: number; total: number; label: string };
/** The steps of a run that are live — running or waiting — in definition order. */
export declare function liveSteps(run: WorkflowRun | null | undefined): WorkflowRun["workflow"]["steps"];
/** The finished banner's sentence: what the run came to — and, failed, at which step and why. */
export declare function finishedWords(run: Pick<WorkflowRun, "outcome" | "workflow" | "steps"> | null | undefined): string;
