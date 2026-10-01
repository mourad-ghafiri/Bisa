/**
 * Types for `settingsSnapshotModel.mjs`, which is plain JavaScript so
 * `node --test` can import it without a build step.
 */

export interface Entry<T> {
  readonly data: readonly T[] | null;
  readonly error: string | null;
  readonly loading: boolean;
  readonly refreshing: boolean;
  readonly at: number | null;
}

export declare const EMPTY_ENTRY: Entry<never>;
export declare function keyOf(project: string | null | undefined): string;
export declare function reading<T>(entry: Entry<T>): Entry<T>;
export declare function settled<T>(entry: Entry<T>, list: readonly T[], at: number): Entry<T>;
export declare function refused<T>(entry: Entry<T>, error: unknown): Entry<T>;
