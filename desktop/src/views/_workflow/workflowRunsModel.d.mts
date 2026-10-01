/** Types for `workflowRunsModel.mjs`. */
import type { EngineEvent, RunStatus, RunSummary, RunView } from "../../types";

export interface RunWordsLike {
  word: string;
  tone: "quiet" | "accent" | "warn" | "ok" | "danger";
  at: number | null;
  atWord: string;
}
export interface RunRowVerbs {
  stop: boolean;
  restart: boolean;
  open: boolean;
}
export interface RunsPaneRow {
  id: string;
  title: string;
  status: RunStatus;
  words: RunWordsLike;
  startedBy: string;
  verbs: RunRowVerbs;
  live: boolean;
}
export declare function isLive(status: string): boolean;
export declare function orderRuns<T extends { status: string }>(runs: readonly T[] | null | undefined): T[];
export declare function runTitle(summary: Pick<RunSummary, "workflow_name" | "number">): string;
export declare function startedBy(summary: Pick<RunSummary, "started_by">): string;
/** How the run's workflow stands now: put away, it starts no run. */
export interface WorkflowStanding {
  archived?: boolean;
}
export declare function runRowVerbs(summary: Pick<RunSummary, "scope" | "status">, workflow?: WorkflowStanding): RunRowVerbs;
export declare function runsPaneRows(runs: readonly RunSummary[] | null | undefined, workflow?: WorkflowStanding): RunsPaneRow[];
/** How many rows the Runs pane draws at first, and how many more each *Show older* adds. */
export declare const RUNS_SHOWN: number;
/** The rows the pane draws out of all it read, what it left out, and what the next *Show older* brings. */
export declare function paneWindow<T extends { live: boolean }>(rows: readonly T[], shown?: number): { rows: T[]; hidden: number; more: number };
export declare function movesRunsOf(event: { workflow?: string | null; payload?: { type?: string } }, workflow: string): boolean;
/** Whether an engine event moves the page of this run: a fact about the run itself, a gate on it, or its workflow deleted. */
export declare function movesRun(event: Pick<EngineEvent, "run" | "payload"> | null | undefined, run: string | null | undefined, workflow: string | null | undefined): boolean;
/** Whether an engine event moves a workflow's row: a run of it — a goal's or the workspace's — started or ended. */
export declare function movesRunCount(event: Pick<EngineEvent, "workflow" | "payload"> | null | undefined, workflow: string): boolean;
/** Where a goal's run opened by its id is handed on. */
export interface RunHandOff {
  route: { name: "goal"; id: string };
  search: { tab: "workflow"; run: string };
}
/** What a run's page decides before it draws. */
export interface RunPageFacts {
  title: string;
  words: RunWordsLike;
  /** Who the run waits on, in words. */
  holder: string;
  startedBy: string;
  /** *rev 3* — the revision the run froze. */
  revision: string;
  verbs: RunRowVerbs;
  finished: boolean;
  /** The steps live right now — the rows that stand open on arrival. */
  live: string[];
  /** The line under the rows once the run ended; `null` while it goes. */
  ended: string | null;
  /** A goal's run: the goal's Workflow tab on this run; `null` for a run of the workspace. */
  handsTo: RunHandOff | null;
}
export declare function runPageFacts(view: RunView | null | undefined): RunPageFacts | null;
/** A press of a run's verb: the set with the run in it, or `null` when its verb is already on its way. */
export declare function taken(flying: ReadonlySet<string>, run: string): Set<string> | null;
/** Whether a verb of this run is on its way. */
export declare function inFlight(flying: ReadonlySet<string>, run: string): boolean;
/** The node answered the run's verb: the run takes one again. */
export declare function settled(flying: ReadonlySet<string>, run: string): ReadonlySet<string>;
/** How a read of runs stands, and the one line it says. */
export interface ReadStanding {
  standing: "reading" | "ready" | "stale" | "failed" | "gone";
  line: string | null;
}
export declare function readStanding(read: { data?: unknown; loading?: boolean; error?: string | null; missing?: boolean; refreshing?: boolean }, what: string): ReadStanding;
