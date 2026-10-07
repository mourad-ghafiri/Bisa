/**
 * Types for `retireModel.mjs`, which is plain JavaScript so `node --test`
 * can import it without a build step.
 */

import type { TerminationCounts } from "./closeWorkstreamModel.mjs";
import type { SessionRow } from "../../types";
import type { TerminalSessionState } from "../../shell/terminalsModel.mjs";

export type RetireKind = "goal" | "workflow";
export type ThingFate = "archive" | "delete";
export type ProjectsFate = "keep" | "archive" | "delete";

export interface RetireProjectFacts {
  id: string;
  slug: string;
  name: string;
  adopted: boolean;
  workstreams: number;
  workstream_ids: string[];
  sessions: number;
  archived: boolean;
}

export interface RetireRunFacts {
  id: string;
  status: string;
  live_steps: number;
}

export interface RetireHolder {
  kind: string;
  id: string;
  label: string;
  live: boolean;
}

export interface RetirePreview {
  agents: number;
  harnesses: number;
  /** The node's wire leaves an empty option out: absent reads as none. */
  run?: RetireRunFacts | null;
  /** A workflow's runs of the workspace that are going. */
  runs: RetireRunFacts[];
  /** How many runs of the workspace a workflow has in all. */
  history: number;
  refusal?: string | null;
  designs: number;
  used_by: RetireHolder[];
  projects_born: RetireProjectFacts[];
  projects_attached: RetireProjectFacts[];
}

/** What is said once it happened: the thing, its fate, and what this app terminated with it. */
export declare function retiredWords(kind: RetireKind, thing: "archive" | "delete", terminated: TerminationCounts | null | undefined, nodeEnded?: import("../../shell/stopOutcomeModel.mjs").StopOutcomeBlock | null): string;
export declare function titleWords(thing: ThingFate, name: string): string;

export interface RetireChoices {
  thing: ThingFate;
  projects: ProjectsFate;
  tree: boolean;
}

export interface RetireSection {
  id: string;
  title: string;
  lines: string[];
  tone: "quiet" | "warn" | "danger";
  holders?: RetireHolder[];
}

export declare function deleteAvailable(preview: RetirePreview | null | undefined): boolean;
export declare function defaultChoices(wanted: ThingFate, preview: RetirePreview | null | undefined): RetireChoices;
export declare function touchedWorkstreams(preview: RetirePreview, choices: RetireChoices): Set<string>;
export declare function terminationOf(
  preview: RetirePreview,
  sessions: readonly SessionRow[] | null | undefined,
  terminals: readonly TerminalSessionState[] | null | undefined,
  choices: RetireChoices,
  goal: string | null,
): TerminationCounts;
export declare function runLine(preview: RetirePreview): string | null;
export declare function runsLine(preview: RetirePreview): string | null;
export declare function historyLine(preview: RetirePreview, choices: RetireChoices): string | null;
export declare function stopsLine(counts: TerminationCounts): string | null;
export declare function retireSections(kind: RetireKind, preview: RetirePreview, choices: RetireChoices, terminated: TerminationCounts): RetireSection[];
export declare function confirmWords(kind: RetireKind, choices: RetireChoices, preview: RetirePreview): string;
export declare function destroys(choices: RetireChoices): boolean;
export declare function planOf(kind: "goal", choices: RetireChoices): { goal: ThingFate; projects: ProjectsFate; tree: boolean };
export declare function planOf(kind: "workflow", choices: RetireChoices): { workflow: ThingFate; projects: ProjectsFate; tree: boolean };
