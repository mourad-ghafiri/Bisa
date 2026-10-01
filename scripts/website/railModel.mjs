/**
 * bisa.dev's tour (website/README.md): its sections follow the app's own
 * sidebar, and the rail in the margin wears the run strip a goal wears in the
 * app — read, being read, still ahead, and the gate at the end that waits on
 * you. These are the rail's rules — and the opening slideshow's — with no DOM in them: `site.src.js` measures
 * and paints, `build.mjs` bundles both into `website/assets/site.js`, and
 * `node --test` reads this.
 */

/** The four states a chip wears: not reached, being read, read, and the gate that waits on you. */
export const STATES = Object.freeze(["pending", "running", "done", "waiting"]);

/**
 * The stop being read: the last section whose top has passed the reading
 * line (a fraction of the viewport down from its top); `-1` before the first
 * has — the page's opening, above the tour.
 * @param {readonly number[]} tops each section's top, relative to the viewport
 * @param {number} line the reading line, in the same pixels
 */
export function currentStep(tops, line) {
  let current = -1;
  tops.forEach((top, i) => {
    if (Number.isFinite(top) && top <= line) current = i;
  });
  return current;
}

/**
 * Each chip's state: those before the current stop are done, the current one
 * runs, the rest wait their turn — and the last, the gate, is marked as
 * waiting on you until the reader reaches it, when it is the one being read.
 * Before the first stop (`-1`, the page's opening) nothing runs yet.
 * @returns {("pending" | "running" | "done" | "waiting")[]}
 */
export function chipStates(count, current) {
  const n = Math.max(0, Math.floor(count));
  const at = Math.min(Math.floor(current), Math.max(0, n - 1));
  return Array.from({ length: n }, (_, i) => {
    if (i === at) return "running";
    if (i === n - 1) return "waiting";
    if (i < at) return "done";
    return "pending";
  });
}

/** The counter beside the rail: `3 of 12`, and nothing before the first stop. */
export function counterWords(current, count) {
  const n = Math.max(1, Math.floor(count));
  if (Math.floor(current) < 0) return "";
  const at = Math.min(Math.floor(current), n - 1);
  return `${at + 1} of ${n}`;
}

/** The slide in view: the track's scroll over a slide's width, rounded, held to the slides there are. */
export function slideAt(scrollLeft, width, count) {
  const n = Math.max(0, Math.floor(count));
  if (n === 0 || !(width > 0) || !Number.isFinite(scrollLeft)) return 0;
  return Math.min(n - 1, Math.max(0, Math.round(scrollLeft / width)));
}

/** The slide that follows: the next one, and the first again after the last. */
export function nextSlide(at, count) {
  const n = Math.max(1, Math.floor(count));
  return (((Math.floor(at) + 1) % n) + n) % n;
}

/** The slide before: the previous one, and the last again before the first. */
export function previousSlide(at, count) {
  const n = Math.max(1, Math.floor(count));
  return (((Math.floor(at) - 1) % n) + n) % n;
}

/** How far through the page the reader is, 0 to 1 — the progress line on a narrow screen. */
export function progress(scrollTop, scrollHeight, viewport) {
  const room = scrollHeight - viewport;
  if (!(room > 0)) return 1;
  return Math.min(1, Math.max(0, scrollTop / room));
}
