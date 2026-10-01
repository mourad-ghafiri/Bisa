/**
 * Types for `auxSlotModel.mjs`, which is plain JavaScript so `node --test`
 * can import it without a build step.
 */

export interface Slot<T> {
  /** The element, or `null` when the pane is gone. The same element twice is silent. */
  publish: (el: T | null | undefined) => void;
  current: () => T | null;
  subscribe: (listener: () => void) => () => void;
}

export declare function createSlot<T>(): Slot<T>;
