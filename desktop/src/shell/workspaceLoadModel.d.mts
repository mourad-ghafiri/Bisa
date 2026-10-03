/**
 * Types for `workspaceLoadModel.mjs`, which is plain JavaScript so
 * `node --test` can import it without a build step.
 */

export interface Settled<T> {
  /** The reads that answered, by name. */
  ok: Record<string, T>;
  /** The reads that did not, each as `name: message`. */
  degraded: string[];
  /** The node itself could not be reached. */
  offline: boolean;
}

export declare function settleLoads<T>(
  names: readonly string[],
  results: readonly PromiseSettledResult<T>[],
  isOffline: (reason: unknown) => boolean,
): Settled<T>;

/** The name a read that did not answer goes under: `name: reason`. */
export declare function failedRead(name: string, reason: unknown): string;
/** The reads that did not answer: the last load's, then the hosted sections' — each list replaced whole by the read that renews it. */
export declare function degradedReads(load: readonly string[], hosted: readonly string[]): string[];
/** A list the node is away for, or whose last read failed: unknown, not empty. */
export declare function listUnread(degraded: readonly string[] | null | undefined, offline: string | null | undefined, name: string): boolean;

export declare function degradedWords(degraded: readonly string[], offline: boolean): { label: string; title: string } | null;

export declare function reloadOnReconnect(
  watch: (cb: (state: "connecting" | "open" | "closed" | "lagged") => void) => () => void,
  reload: () => void,
): () => void;

export declare function hostedFailure(hostLabel: string, what: "channels" | "dms", reason: unknown): string;
/** The shell's line while the node cannot be reached. */
export declare function offlineWords(reason: string | null): string;
/** The shell's line while the node is away — by the last load's word or the bus's — `null` while it is there. */
export declare function offlineLine(loadSaid: string | null, conn: "connecting" | "open" | "closed" | "lagged", reason: string | null): string | null;
export declare function reconnectWords(): string;
export declare const RELOADS_WORKSPACE: readonly string[];
export declare function reloadsWorkspace(type: unknown): boolean;
