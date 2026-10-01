/** Types for `listeningModel.mjs`. */
import type { Budget, InputDef, ListenerView, Listening, Paused, StartSummary, WorkflowRow } from "../../types";

export type SwitchStateName = "none" | "off" | "blocked" | "on" | "paused";

export interface SwitchState {
  state: SwitchStateName;
  words: string;
  tone: "quiet" | "ok" | "warn" | "danger";
  /** What pressing the switch does: open the Turn on dialog, turn it off, or nothing. */
  toggle: "on" | "off" | null;
  /** Paused: it may be heard again with the inputs it listened with. */
  again: boolean;
}

export interface GoalListeningLine {
  paused: boolean;
  words: string;
  tone: "accent" | "warn";
}

export interface TurnOnBody {
  inputs?: Record<string, unknown>;
  budget?: Budget;
}

export declare const NEXT_DUE_FORMAT: Readonly<Intl.DateTimeFormatOptions>;
export declare function eventStartsOf(row: Pick<WorkflowRow, "starts"> | null | undefined): StartSummary[];
export declare function hasEventStarts(row: Pick<WorkflowRow, "starts"> | null | undefined): boolean;
export declare function listeningFor(row: Pick<WorkflowRow, "starts"> | null | undefined): string;
export declare function nextDue(listeners: readonly Pick<ListenerView, "next_due">[] | null | undefined): number | null;
export declare function nextWords(at: number): string;
export declare function pausedWords(paused: Paused | null | undefined): string;
export declare function turnOnBlockers(row: WorkflowRow | null | undefined): string[];
export declare function switchState(row: WorkflowRow | null | undefined, listeners?: readonly ListenerView[] | null): SwitchState;
export declare function neededInputs(row: WorkflowRow | null | undefined): InputDef[];
export declare function turnOnBody(inputs: Record<string, unknown> | null | undefined, budget: Partial<Record<"max_usd_cents" | "max_tokens" | "max_wall_clock_secs", number | string | null>> | null | undefined): TurnOnBody;
/** A per-run ceiling as a person types it: dollars, tokens, minutes — blank for none. */
export interface BudgetDraft {
  dollars: string;
  tokens: string;
  minutes: string;
}
/** The draft read: the node's budget — cents, tokens, seconds — and a sentence per field that is no ceiling. */
export interface BudgetRead {
  budget: { max_usd_cents: number | null; max_tokens: number | null; max_wall_clock_secs: number | null };
  errors: Partial<Record<keyof BudgetDraft, string>>;
}
export declare function readBudget(draft: Partial<BudgetDraft> | null | undefined): BudgetRead;
export declare function againBody(listening: Listening | null | undefined): TurnOnBody;
export declare function goalListening(listening: Listening | null | undefined, listeners?: readonly ListenerView[] | null): GoalListeningLine | null;
export declare function movesListeningOf(event: { payload?: { type?: string } } | null | undefined, host: string): boolean;
