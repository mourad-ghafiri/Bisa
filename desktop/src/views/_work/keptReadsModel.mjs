/**
 * What a screen read last, kept for the life of the window — so a screen
 * that comes back draws its last answer at once and reads again behind it,
 * instead of standing empty until the node answers. A read is the node's
 * word, not the window's: nothing here is written to storage, and after a
 * restart the node is read.
 *
 * Bounded (`shell/lru.mjs`): the reads of the screens a person is moving
 * between, never every read of a weeks-long window. Facts only, no React
 * (`keptReadsStore.ts` keeps them). Plain `.mjs`, so `node --test` reads it.
 */

import { Lru } from "../../shell/lru.mjs";

/** How many reads one window keeps — the newest used kept. */
export const MAX_READS = 96;

/**
 * The key a read is kept under: what is read, and of which thing —
 * `goal:01G`, `workflows`. A part that is nothing makes no key: a read of
 * nothing in particular is not worth keeping.
 * @param {string} what
 * @param {...(string | null | undefined)} of
 * @returns {string | null}
 */
export function readKey(what, ...of) {
  if (!what) return null;
  for (const part of of) if (part === null || part === undefined || part === "") return null;
  return [what, ...of].join(":");
}

/** The reads kept: a bounded map, newest used last. */
export class KeptReads {
  /** @param {number} [capacity] */
  constructor(capacity = MAX_READS) {
    this.reads = new Lru(capacity);
  }

  /** What was read under `key`, or `undefined`. @param {string | null | undefined} key */
  read(key) {
    return key ? this.reads.get(key) : undefined;
  }

  /** Keep an answer; an answer that is nothing keeps nothing. @param {string | null | undefined} key @param {unknown} answer */
  keep(key, answer) {
    if (!key || answer === undefined || answer === null) return;
    this.reads.set(key, answer);
  }

  /** Forget one read. @param {string | null | undefined} key */
  forget(key) {
    if (key) this.reads.delete(key);
  }

  /** Forget every read — *Forget where I was*, another workspace. */
  clear() {
    for (const key of this.reads.keys()) this.reads.delete(key);
  }

  /** How many reads are kept. */
  get size() {
    return this.reads.size;
  }
}
