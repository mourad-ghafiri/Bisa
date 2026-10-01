/**
 * The footer's resources as facts: three parts — the host, the processes, the
 * disk — each compared on its own so a reading that did not move re-renders
 * nothing, and a reading that failed is *kept* and marked stale rather than
 * blanked. `statsStore.ts` polls and notifies; the rules about what a reading
 * changes live here, where `node --test` can step them
 * (`scenarios/resources.test.mjs`).
 */

import { sameJsonList } from "./snapshotEqual.mjs";

/** No host yet: nothing read, nothing stale. */
export const EMPTY_HOST = Object.freeze({ info: null, load: null, stale: null });
/** No process rows yet. */
export const EMPTY_PROCESSES = Object.freeze({ processes: Object.freeze([]), intervalSecs: null, readMs: null });
/** No disk sizes yet, and the data directory not yet known. */
export const EMPTY_DISK = Object.freeze({ disk: null, dataDir: null });

/**
 * Whether two parts say the same thing — plain JSON, so structural.
 * @param {unknown} a
 * @param {unknown} b
 */
export function samePart(a, b) {
  return JSON.stringify(a) === JSON.stringify(b);
}

/**
 * The machine's facts landed — read once; the load and the mark are untouched.
 * @template {{ info: unknown; load: unknown; stale: string | null }} H
 * @param {H} host
 * @param {unknown} info
 * @returns {H}
 */
export function withInfo(host, info) {
  return { ...host, info };
}

/**
 * A reading landed: it is the load now, and whatever was stale is not.
 * @template {{ info: unknown; load: unknown; stale: string | null }} H
 * @param {H} host
 * @param {unknown} load
 * @returns {H}
 */
export function afterReading(host, load) {
  return { ...host, load, stale: null };
}

/**
 * A reading failed: the last reading stands, marked with why. The bar keeps
 * drawing numbers — a blank would read as "nothing running".
 * @template {{ info: unknown; load: unknown; stale: string | null }} H
 * @param {H} host
 * @param {string} why
 * @returns {H}
 */
export function afterFailure(host, why) {
  return { ...host, stale: why };
}

/**
 * Whether a failure is news — the first time, or a different reason — so the
 * log says it once per reason and not once per tick.
 * @param {{ stale: string | null }} host
 * @param {string} why
 */
export function failureIsNews(host, why) {
  return host.stale !== why;
}

/**
 * The processes after a read. `readMs` and the interval move on every read
 * and are not a change; the rows are compared one by one. Answers the part to
 * hold and whether readers must hear about it.
 * @template {{ processes: readonly unknown[]; intervalSecs: number | null; readMs: number | null }} P
 * @param {P} procs
 * @param {P} next
 * @returns {{ procs: P; changed: boolean }}
 */
export function processesAfter(procs, next) {
  if (sameJsonList(next.processes, procs.processes) && next.intervalSecs === procs.intervalSecs) {
    return { procs: { ...procs, readMs: next.readMs }, changed: false };
  }
  return { procs: next, changed: true };
}
