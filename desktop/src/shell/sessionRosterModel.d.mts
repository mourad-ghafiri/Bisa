/**
 * Types for `sessionRosterModel.mjs`, which is plain JavaScript so
 * `node --test` can import it without a build step.
 */

/** The roster with one row as a frame says it: replaced where it stood, else first. */
export declare function upserted<R extends { id: string }>(sessions: readonly R[], row: R): R[];
/** The roster without a row; the same array when it holds none. */
export declare function dropped<R extends { id: string }>(sessions: readonly R[], id: string): readonly R[];
/** A read of the whole roster as it lands: the node's rows, under every frame that arrived while the read was out. */
export declare function landedRead<R extends { id: string }>(snapshot: readonly R[], since: ReadonlyMap<string, R | null>): R[];
/** Whether a *Stop* that threw found no such session — the node's 404. */
export declare function stoppedAlready(error: unknown): boolean;
/** What a *Stop* did: the node stopped the session, or there was none left to stop. */
export type StopOutcome = "stopped" | "gone";
/** The toast after a *Stop*: the surface's sentence, or the plain fact that the session had already ended. */
export declare function stopWords(outcome: StopOutcome, stopped: string): string;
