import type { DecisionPoint, PointStatus, DecisionProviderKind, DecisionRequest, DecisionResponse, DeciderStatus, JudgementRecord } from "../../types";

export type Tone = "ok" | "warn" | "danger" | "quiet";

export declare const KEYS: {
  enabled: string;
  pointsOff: string;
  provider: string;
  harness: { id: string; model: string; effort: string };
  agent: { id: string };
  jev: { model: string };
  rlcd: { endpoint: string; model: string; auth: string };
  deadline: string;
  retries: string;
  confidenceAct: string;
  confidenceSecurity: string;
};
export declare const SCALARS: readonly string[];

export interface PointWords {
  label: string;
  description: string;
}
export declare const POINTS: Readonly<Record<DecisionPoint, PointWords>>;
export declare const POINT_IDS: readonly DecisionPoint[];
export declare function pointLabel(point: string | null | undefined): string;

export declare function fieldsFor(provider: DecisionProviderKind | string, rlcdAuth?: string): string[];
export declare function takesKey(provider: DecisionProviderKind | string, rlcdAuth?: string): boolean;
export declare function pointsOffAfter(current: readonly string[] | null | undefined, point: string, on: boolean): string[];
export declare function pointIsOn(row: PointStatus | null | undefined): boolean;

/** The list a switch is built on: the last one this window wrote, until the node's read has caught up. */
export declare function offList(read: unknown, written: readonly string[] | null | undefined): string[];
/** One decision point as the panel draws it. */
export interface PointRow {
  id: DecisionPoint;
  label: string;
  hint: string;
  on: boolean;
  /** Selected by name where it is used: on whatever the switches say. */
  selected: boolean;
  /** Its switch is not the person's to flip right now. */
  held: boolean;
}
export declare function pointRows(status: Pick<DeciderStatus, "enabled" | "points"> | null | undefined, writing: string | null): PointRow[];
/** A provider's API key as its box holds it — the box is its provider's own. */
export interface KeyBox {
  provider: string;
  typed: string;
  /** What the node already holds of it; `null` for nothing sent from here. */
  sent: string | null;
}
export declare function blankKey(provider: string): KeyBox;
export declare function keyFor(box: KeyBox | null | undefined, provider: string): KeyBox;
export declare function keyTyped(box: KeyBox | null | undefined, provider: string, typed: string): KeyBox;
export declare function keySaved(box: KeyBox | null | undefined, provider: string): KeyBox;
export declare function keyCleared(provider: string): KeyBox;
export declare function maySaveKey(box: KeyBox | null | undefined, provider: string): boolean;
export declare function mayClearKey(status: Pick<DeciderStatus, "key_stored"> | null | undefined): boolean;
export declare function movesStatus(payload: { type?: string; keys?: readonly string[] } | null | undefined): boolean;
export declare function movesJudgements(payload: { type?: string } | null | undefined): boolean;
export declare function readyLine(status: DeciderStatus | null | undefined): { tone: Tone; text: string };
export declare function calibratedNote(status: DeciderStatus | null | undefined): string | null;

export interface TryOption {
  id: string;
  meaning: string;
}
export interface TryForm {
  state: string;
  kind: "noul" | "choice";
  instructions: string;
  options: TryOption[];
}
export declare function blankTryForm(): TryForm;
export declare function tryProblem(form: TryForm): string | null;
export declare function tryRequest(form: TryForm): DecisionRequest;
export declare function answerLines(response: DecisionResponse | null | undefined): string[];
export declare function judgementLine(record: JudgementRecord, now?: number): string;
export declare function coreLine(status: DeciderStatus | null | undefined, error?: string | null): string;
