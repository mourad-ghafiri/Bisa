/**
 * Types for `verbLatch.mjs`. This file is the only reason TypeScript never
 * has to read it.
 */

export interface Latch {
  /** Take `key`: true when it was free, false while it is held. */
  take(key: string): boolean;
  release(key: string): void;
  holds(key: string): boolean;
}
export declare function createLatch(): Latch;
/** Run `send` under `key`, once while it is in flight. Answers whether this call sent. */
export declare function once(latch: Latch, key: string, send: () => Promise<unknown>): Promise<boolean>;
/** The key of a set of uploads: their hashes, in order. */
export declare function uploadsKey(uploaded: readonly { sha256?: string }[] | null | undefined): string;
