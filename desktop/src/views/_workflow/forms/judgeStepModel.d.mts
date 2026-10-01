/** Types for `judgeStepModel.mjs`. */
import type { Step } from "../../../types";

type Judge = Extract<Step, { kind: "judge" }>;

export type ConfidenceRead = { ok: true; value: number | null } | { ok: false; reason: string };
export type ConfidenceSet = { ok: true; step: Judge } | { ok: false; reason: string };

/** A judgement chooses between at least this many options. */
export declare const MIN_OPTIONS: number;
export declare function confidenceText(step: Pick<Judge, "min_confidence"> | null | undefined): string;
export declare function readConfidence(text: string): ConfidenceRead;
export declare function setConfidence(step: Judge, text: string): ConfidenceSet;
export declare function addOption(step: Judge): Judge;
export declare function setMeaning(step: Judge, index: number, meaning: string): Judge;
export declare function removeOption(step: Judge, index: number): Judge;
export declare function judgeWords(step: Judge | null | undefined): { options: string | null; otherwise: string };
