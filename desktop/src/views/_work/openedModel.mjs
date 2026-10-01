/**
 * The rows a person opened on a list — a feed's rows, an Inbox row's
 * notices, a run's steps — as a screen keeps them (`shell/viewMemoryStore`):
 * a set on screen, its words in the memory. One rule for every list that
 * opens rows, so what is opened on one comes back the way it does on the
 * others. Facts only, no React. Plain `.mjs`, so `node --test` reads it.
 */

import { wordsValue } from "../../shell/viewValuesModel.mjs";

/** How many opened rows a list keeps — the newest opened kept. */
export const MAX_OPENED = 64;

/**
 * The opened rows read back from the memory, or `undefined` for what is no
 * list of rows — which the screen reads as none opened.
 * @param {unknown} raw
 * @returns {Set<string> | undefined}
 */
export function parseOpened(raw) {
  const words = wordsValue(raw);
  return words ? new Set(words.slice(-MAX_OPENED)) : undefined;
}

/**
 * The rows after one was pressed: opened when it was closed, closed when it
 * was opened. A new set either way; past the cap the row opened longest ago
 * is closed.
 * @param {ReadonlySet<string>} opened
 * @param {string} key
 * @param {number} [cap]
 * @returns {Set<string>}
 */
export function toggled(opened, key, cap = MAX_OPENED) {
  const next = new Set(opened);
  if (next.delete(key)) return next;
  next.add(key);
  while (next.size > Math.max(1, cap)) {
    const oldest = next.values().next().value;
    if (oldest === undefined) break;
    next.delete(oldest);
  }
  return next;
}
