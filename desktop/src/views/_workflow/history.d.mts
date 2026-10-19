/**
 * Types for `history.mjs`. This file is the only reason TypeScript never has
 * to read it.
 */

/** One entry that left the head: its value and the selection it was left with. */
export interface HistoryEntry<T, S = unknown> {
  value: T;
  selection: S;
}

export interface History<T, S = unknown> {
  past: HistoryEntry<T, S>[];
  present: T;
  future: HistoryEntry<T, S>[];
  capacity: number;
  /** When the head was pushed, in ms since the epoch; 0 for a head that never coalesces. */
  at: number;
  /** What made the head — the coalescing key — or none. */
  token: string | null;
  /** The selection the last undo or redo landed on; `null` otherwise. */
  landed: S | null;
}

/** What a push says about itself: the selection while it was made, when, and what it typed into. */
export interface Edit<S = unknown> {
  selection?: S;
  at?: number;
  token?: string | null;
}

export declare const COALESCE_MS: number;
export declare function create<T, S = unknown>(initial: T, capacity?: number): History<T, S>;
export declare function push<T, S = unknown>(h: History<T, S>, next: T, edit?: Edit<S>): History<T, S>;
export declare function undo<T, S = unknown>(h: History<T, S>, selection?: S): History<T, S>;
export declare function redo<T, S = unknown>(h: History<T, S>, selection?: S): History<T, S>;
export declare function canUndo<T, S = unknown>(h: History<T, S>): boolean;
export declare function canRedo<T, S = unknown>(h: History<T, S>): boolean;
