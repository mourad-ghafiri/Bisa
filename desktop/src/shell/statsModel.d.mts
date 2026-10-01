/**
 * Types for `statsModel.mjs`, plain JavaScript so `node --test` can step it.
 * The parts are generic over the wire shapes the store fills them with.
 */

export interface HostState<I, L> {
  readonly info: I | null;
  readonly load: L | null;
  /** Why the last read failed, while the last reading stands; `null` while the reads answer. */
  readonly stale: string | null;
}

export interface ProcessesState<R> {
  readonly processes: readonly R[];
  readonly intervalSecs: number | null;
  readonly readMs: number | null;
}

export interface DiskState<D> {
  readonly disk: D | null;
  readonly dataDir: string | null;
}

export declare const EMPTY_HOST: HostState<never, never>;
export declare const EMPTY_PROCESSES: ProcessesState<never>;
export declare const EMPTY_DISK: DiskState<never>;

export declare function samePart(a: unknown, b: unknown): boolean;
export declare function withInfo<I, L>(host: HostState<I, L>, info: I): HostState<I, L>;
export declare function afterReading<I, L>(host: HostState<I, L>, load: L): HostState<I, L>;
export declare function afterFailure<I, L>(host: HostState<I, L>, why: string): HostState<I, L>;
export declare function failureIsNews(host: { stale: string | null }, why: string): boolean;
export declare function processesAfter<R>(procs: ProcessesState<R>, next: ProcessesState<R>): { procs: ProcessesState<R>; changed: boolean };
