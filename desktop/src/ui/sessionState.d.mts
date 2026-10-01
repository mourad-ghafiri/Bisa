import type { SessionState, SubagentPresence } from "../types";

export type SessionWord = "waiting" | "failed" | "aborted" | "done" | "thinking" | "running" | "starting" | "idle" | "parked";
export type SessionTone = "accent" | "danger" | "ok" | "working" | "dim";
export interface StateDescription {
  label: string;
  tone: SessionTone;
  icon: string;
  rank: number;
}
export interface Counts {
  waiting: number;
  working: number;
  done: number;
  failed: number;
  live: number;
}
type StateLike = SessionState | SessionWord | null | undefined;
interface RowLike {
  state: SessionState;
  since?: number;
  children?: readonly SubagentPresence[];
}

export declare const STATES: readonly SessionWord[];
export declare function stateOf(state: StateLike): SessionWord;
export declare function describe(state: StateLike): StateDescription;
export declare function attentionRank(state: StateLike): number;
export declare function toneOf(state: StateLike): SessionTone;
export declare function iconOf(state: StateLike): string;
export declare function isLive(state: StateLike): boolean;
export declare function isStoppable(state: StateLike): boolean;
export declare function isAttention(state: StateLike): boolean;
export declare function isEnded(state: StateLike): boolean;
export declare function label(state: StateLike): string;
/** What a waiting session waits on, in its own words, with no *waiting on you* before them; `null` when it says no more than that it waits. */
export declare function waitWords(state: unknown): string | null;
export declare function liveChildren<T extends { state: SessionState }>(row: { state: SessionState; children?: readonly T[] } | null | undefined): T[];
export declare function counts(rows: readonly RowLike[] | null | undefined): Counts;
export declare function loudest(rows: readonly RowLike[] | null | undefined): SessionWord;
export declare function sortRows<T extends RowLike>(rows: readonly T[] | null | undefined): T[];
export declare function isNotifiable(prev: StateLike, next: StateLike): boolean;
export declare function gateOf(state: StateLike): string | null;
