/**
 * Types for `sessionRosterModel.mjs`, which is plain JavaScript so
 * `node --test` can import it without a build step.
 */

/** The newer of two words about one row, by the roster's `revision`; the later word when either has none. */
export declare function latest<R extends { revision?: number }>(held: R, said: R): R;
/** The roster with one row as a frame says it: replaced where it stood, else first; the same array for a frame older than the row held. */
export declare function upserted<R extends { id: string; revision?: number }>(sessions: readonly R[], row: R): readonly R[];
/** The roster without a row; the same array when it holds none. */
export declare function dropped<R extends { id: string }>(sessions: readonly R[], id: string): readonly R[];
/** A read of the whole roster as it lands beside the frames that arrived while it was out: the newer word of each row by `revision`, a gone row staying gone. */
export declare function landedRead<R extends { id: string; revision?: number }>(snapshot: readonly R[], since: ReadonlyMap<string, R | null>): R[];
/** Whether a *Stop* that threw found no such session — the node's 404. */
export declare function stoppedAlready(error: unknown): boolean;
/** What a *Stop* did: the node stopped the session, or there was none left to stop. */
export type StopOutcome = "stopped" | "gone";
/** The toast after a *Stop*: the surface's sentence — and after it the node's word on what the stop ended — or the plain fact that the session had already ended. */
export declare function stopWords(outcome: StopOutcome, stopped: string, ended?: import("./stopOutcomeModel.mjs").StopOutcomeBlock | null): string;
/** The rows a read of the whole roster moved, each as the transition a frame would have announced. */
export declare function transitionsBetween<R extends { id: string; state: { state: string } }>(before: readonly R[], after: readonly R[]): [R["state"] | null, R][];
