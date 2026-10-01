/**
 * Types for `history.mjs`. This file is the only reason TypeScript never has
 * to read it.
 */

export interface History<T> {
  past: T[];
  present: T;
  future: T[];
  capacity: number;
}

export declare function create<T>(initial: T, capacity?: number): History<T>;
export declare function push<T>(h: History<T>, next: T): History<T>;
export declare function undo<T>(h: History<T>): History<T>;
export declare function redo<T>(h: History<T>): History<T>;
export declare function canUndo<T>(h: History<T>): boolean;
export declare function canRedo<T>(h: History<T>): boolean;
