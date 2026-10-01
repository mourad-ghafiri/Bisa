/**
 * What a screen's memory gives back, made safe to draw: the parsers
 * `useViewState` is handed (`viewMemoryStore.ts`). The memory is window
 * furniture — it may have been written by another version of a screen, or
 * by hand — so a value is what its parser knows, else `undefined`, which
 * the hook reads as the screen's own beginning. Facts only. Plain `.mjs`,
 * so `node --test` reads it.
 */

/** The longest text a memory gives back; a longer one is cut, never refused. */
export const MAX_TEXT = 512;
/** The most words a list gives back; the rest are dropped. */
export const MAX_WORDS = 500;

/** A text a person typed — a search box, a filter. @param {unknown} raw @returns {string | undefined} */
export function textValue(raw) {
  return typeof raw === "string" ? raw.slice(0, MAX_TEXT) : undefined;
}

/** A switch. @param {unknown} raw @returns {boolean | undefined} */
export function flagValue(raw) {
  return typeof raw === "boolean" ? raw : undefined;
}

/** A whole number that is not negative — a depth, a count. @param {unknown} raw @returns {number | undefined} */
export function countValue(raw) {
  return typeof raw === "number" && Number.isInteger(raw) && raw >= 0 ? raw : undefined;
}

/**
 * A parser for one word of a vocabulary: a word off the list is nothing.
 * @template {string} W
 * @param {readonly W[]} offered
 * @returns {(raw: unknown) => W | undefined}
 */
export function wordOf(offered) {
  return (raw) => (typeof raw === "string" && offered.includes(/** @type {W} */ (raw)) ? /** @type {W} */ (raw) : undefined);
}

/** An id, or nothing: a text with something in it. @param {unknown} raw @returns {string | undefined} */
export function idValue(raw) {
  return typeof raw === "string" && raw.length > 0 && raw.length <= MAX_TEXT ? raw : undefined;
}

/**
 * A list of words — the rows opened, the tags picked — each one once, in
 * the order kept; what is not a text is left out.
 * @param {unknown} raw
 * @returns {string[] | undefined}
 */
export function wordsValue(raw) {
  if (!Array.isArray(raw)) return undefined;
  const out = [];
  const seen = new Set();
  for (const word of raw) {
    if (typeof word !== "string" || word.length === 0 || word.length > MAX_TEXT || seen.has(word)) continue;
    seen.add(word);
    out.push(word);
    if (out.length >= MAX_WORDS) break;
  }
  return out;
}

/**
 * A set as it is kept: its words as a list, in the set's order.
 * @param {Iterable<string>} set
 * @returns {string[]}
 */
export function wordsOf(set) {
  return [...set].slice(0, MAX_WORDS);
}

/**
 * Whether two values are kept as the same text — what says a value is the
 * screen's own beginning, and so worth no memory.
 * @param {unknown} a
 * @param {unknown} b
 */
export function sameKept(a, b) {
  try {
    return JSON.stringify(a) === JSON.stringify(b);
  } catch {
    return false;
  }
}
