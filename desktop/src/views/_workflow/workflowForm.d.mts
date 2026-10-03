/**
 * Types for `workflowForm.mjs`. This file is the only reason TypeScript never
 * has to read it.
 */

import type { InputDef, Step } from "../../types";

export type InputValues = Record<string, unknown>;

export declare function initialValues(inputs: readonly InputDef[] | null | undefined): InputValues;
export declare function validateInputs(
  inputs: readonly InputDef[] | null | undefined,
  values: InputValues | null | undefined,
): Record<string, string>;
export declare function toRequest(
  inputs: readonly InputDef[] | null | undefined,
  values: InputValues | null | undefined,
): Record<string, unknown>;
/** Whom a run is for: a goal, or the workspace. */
export type RunHome = "goal" | "workspace";
/** The line under an input's field: required or not, what picking a project does, and what is wrong. */
export declare function inputHint(def: Pick<InputDef, "kind" | "required">, error: string | null | undefined, home: RunHome): string;
export declare function accountChoices(accounts: readonly { id: string; label: string; default?: boolean }[] | null | undefined): { id: string; label: string; isDefault: boolean }[];
export declare function nextInputName(inputs: readonly InputDef[] | null | undefined): string;
export declare const INPUT_KINDS: readonly InputDef["kind"][];
/** An input kind in words, never its wire slug. */
export declare function inputKindWords(kind: InputDef["kind"] | string): string;
export type IdProblem = "grammar" | "taken";
/** Why an id or a name as typed cannot be taken, or `null` when it may. */
export declare function idProblem(draft: string, current: string, taken: (name: string) => boolean): IdProblem | null;
/** The reason in words — and, with `kept`, that the old value was put back. */
export declare function idProblemWords(problem: IdProblem | null, what: "step" | "input", kept?: string | null): string | null;
export declare function optionsText(options: readonly string[] | null | undefined): string;
export declare function optionsFrom(text: string): string[];
