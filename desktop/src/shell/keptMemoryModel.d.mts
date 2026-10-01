/** Types for `keptMemoryModel.mjs`. */

/** How much a memory holds. */
export interface KeptCaps {
  /** The most places kept; the oldest is forgotten past it. */
  places: number;
  /** The most text one value may be written as; a longer one lives for the window. */
  valueBytes: number;
  /** The most text the whole memory is written as, newest places first. */
  totalBytes: number;
}

/** The clock and the storage, given by the caller — the window's, or a test's. */
export interface KeptHands {
  storage: () => Storage | null;
  now: () => number;
  later: (fn: () => void, ms: number) => unknown;
  cancel: (handle: unknown) => void;
}

export interface KeptMemoryOptions {
  key: string;
  version: number;
  caps: KeptCaps;
  hands: KeptHands;
  beat?: number;
  atMost?: number;
}

export declare const WRITE_BEAT_MS: number;
export declare const WRITE_AT_MOST_MS: number;

export declare class KeptMemory {
  constructor(options: KeptMemoryOptions);
  readonly key: string;
  readonly version: number;
  /** The workspace the memory is for, once one was adopted. */
  readonly owner: string | null;
  /** When the storage last took a write, by the hands' clock; 0 before any. */
  readonly writtenAt: number;
  read(place: string, name: string): unknown;
  knows(place: string): boolean;
  keptPlaces(): string[];
  keep(place: string, name: string, value: unknown): boolean;
  keepQuietly(place: string, name: string, value: unknown): boolean;
  forget(place: string): boolean;
  forgetUnder(prefix: string): number;
  move(from: string, to: string): boolean;
  clear(): void;
  /** Take nothing more and write nothing more, for the rest of this window. */
  seal(): void;
  /** Whether the memory was sealed. */
  readonly sealed: boolean;
  adopt(owner: string | null | undefined): boolean;
  subscribe(place: string, name: string, listener: () => void): () => void;
  serialized(): string | null;
  flush(): boolean;
  waiting(): boolean;
}

export declare function settleLeft(writtenAt: number, now: number, settle: number): number;
