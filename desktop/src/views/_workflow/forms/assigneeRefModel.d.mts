/** Types for `assigneeRefModel.mjs`. */
import type { Assignee } from "../../../types";

/** An assignee a step names: a fixed one, or an input read when the run starts. */
export type AssigneeRef = Assignee | { input: string };
/** What a field that takes one holds: the input, the picker's word, or nothing. */
export type RefValue = { input: string } | string | null;

export declare function wordOf(ref: AssigneeRef | null | undefined): string | null;
export declare function refOf(word: string): Assignee | null;
export declare function valueOf(ref: AssigneeRef | null | undefined): RefValue;
export declare function picked(value: RefValue | undefined): AssigneeRef | null;
export declare function pickedAgent(value: RefValue | undefined): { agent: string } | { input: string } | null;
export declare function fixedWords(refs: readonly AssigneeRef[] | null | undefined): string[];
export declare function inputNames(refs: readonly AssigneeRef[] | null | undefined): string[];
export declare function withFixed(refs: readonly AssigneeRef[] | null | undefined, words: readonly string[]): AssigneeRef[];
export declare function toggleInput(refs: readonly AssigneeRef[] | null | undefined, name: string): AssigneeRef[];
