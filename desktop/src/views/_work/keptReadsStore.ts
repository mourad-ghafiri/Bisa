/**
 * The reads kept for the life of the window (`keptReadsModel.mjs` has the
 * rules): what `useAsync` draws at once for a screen that comes back, and
 * what the screens with a loader of their own — the Inbox's rows, the
 * Pulse's pages — keep the same way. Memory only: a read is the node's
 * word, and after a restart the node is read.
 */

import { forgetThreads } from "../_studio/threadsStore";
import { KeptReads } from "./keptReadsModel.mjs";

const reads = new KeptReads();

/** What was read under `key` in this window, or `undefined`. */
export function keptRead<T>(key: string | null | undefined): T | undefined {
  return reads.read(key) as T | undefined;
}

/** Keep an answer under `key`. */
export function keepRead(key: string | null | undefined, answer: unknown): void {
  reads.keep(key, answer);
}

/** A thing is gone: its read with it. */
export function forgetRead(key: string | null | undefined): void {
  reads.forget(key);
}

/** Forget every read — *Forget where I was*, another workspace — and the threads' messages, which are reads kept the same way. */
export function forgetEveryRead(): void {
  reads.clear();
  forgetThreads();
}
