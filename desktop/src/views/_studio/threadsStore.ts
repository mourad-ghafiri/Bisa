/**
 * The messages each thread had, kept for the life of the window
 * (`threadCacheModel.mjs` has the rules): what `useScopeMessages` draws at
 * once for a thread that comes back, so the message a person was reading is
 * in the page before the node answers. Memory only — a message is the node's
 * word, and after a restart the node is read — and bounded: the newest
 * threads shown, the newest messages of each.
 *
 * Nothing here hears the bus and nothing subscribes: a thread is read once,
 * when its hook mounts, and the hook — which does hear the bus, and reads
 * again when it comes back — writes what it holds.
 */

import { Lru } from "../../shell/lru.mjs";
import type { MessageRow, ReactionRow } from "../../types";
import { MAX_THREADS, keptThread, type HeldThread } from "./threadCacheModel.mjs";

/** A thread as it is kept: its messages oldest first, the reactions on them, and whether more stands above. */
export type KeptThread = HeldThread<MessageRow, ReactionRow>;

const threads = new Lru<KeptThread>(MAX_THREADS);

/** What a thread held when it was last on screen in this window, or `undefined`. */
export function threadOf(key: string | null | undefined): KeptThread | undefined {
  return key ? threads.get(key) : undefined;
}

/** Keep what a thread holds — the newest of it, past the cap. A thread with nothing in it keeps nothing. */
export function keepThread(key: string | null | undefined, thread: KeptThread): void {
  if (!key) return;
  if (thread.messages.length === 0) {
    threads.delete(key);
    return;
  }
  threads.set(key, keptThread(thread));
}

/** Forget every thread — *Forget where I was*, another workspace. */
export function forgetThreads(): void {
  for (const key of threads.keys()) threads.delete(key);
}
