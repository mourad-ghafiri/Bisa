/** Types for `runDialogModel.mjs`. */
import type { Step, Workflow } from "../../types";
import type { InputValues } from "./workflowForm.mjs";

/** The entry *By hand*: no step id reads so. */
export declare const BY_HAND: "*";

/** One way in the dialog offers. */
export interface RunEntry {
  /** `BY_HAND`, or the id of an event start. */
  id: string;
  /** A test run: it begins at an event start, as if its event had happened. */
  test: boolean;
  label: string;
  /** What the start begins on, in words; none by hand. */
  hint: string | null;
}
/** What an entry asks of the person, and how many inputs its event fills instead. */
export interface AskedInputs {
  asked: string[];
  mapped: number;
}
export type PayloadRead = { ok: true; event: unknown } | { ok: false; error: string };
/** What the dialog sends, or why it sends nothing. */
export type RunRequest =
  | { kind: "run"; inputs: Record<string, unknown> }
  | { kind: "test"; body: { inputs: Record<string, unknown>; start: string; event: unknown } }
  | { kind: "refused"; errors: Record<string, string>; payloadError: string | null };
/** A start the node refused, in words. */
export interface RunRefusal {
  words: string;
  /** Refused by name: a step reads the goal it serves. */
  needsGoal: boolean;
  /** The row the card drew is older than the workflow: read it again. */
  reread: boolean;
}

type Definition = Pick<Workflow, "inputs" | "steps">;

export declare function runEntries(workflow: Definition): RunEntry[];
export declare function firstEntry(workflow: Definition): string | null;
export declare function entryStart(workflow: Definition, entry: string): Extract<Step, { kind: "start" }> | null;
export declare function askedInputs(workflow: Definition, entry: string): AskedInputs;
export declare function sampleText(workflow: Definition, entry: string, now?: number): string;
export declare function payloadOf(text: string): PayloadRead;
export declare function runRequest(workflow: Definition, entry: string, values: InputValues, payload: string): RunRequest;
export declare function runRefusal(body: unknown, message: string): RunRefusal;
