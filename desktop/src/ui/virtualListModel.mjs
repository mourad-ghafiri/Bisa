/**
 * The window a virtual list paints — no React, no DOM. `VirtualList.tsx`
 * measures its viewport and scroll and hands the numbers here; what comes
 * back is the slice of rows to draw and the spacer that stands for the
 * rest. The rules that matter:
 *
 * - An **unmeasured viewport draws no window** — never a guessed one. A
 *   list that painted twenty rows because it did not know its height was a
 *   list cut at the bottom of every panel taller than that.
 * - The row under the viewport's **bottom edge is always inside** the
 *   window, whatever the count did since the last scroll.
 * - When the total shrinks under a deep scroll, the scroll is **clamped**
 *   into it, so the window never starts past the last row.
 * - An **unbounded** list — as tall as its rows, inside a panel that scrolls
 *   as one — windows against that panel: its scroll, its height, and where
 *   the list starts in it (`localFrame`).
 *
 * Plain `.mjs`, so `node --test` reads it.
 */

/**
 * First index whose *end* is past `y`. Offsets are ascending, so: bisect.
 * `offsets[i]` is the top of row `i`; the last entry is the total height.
 * @param {readonly number[]} offsets
 * @param {number} y
 */
export function indexAt(offsets, y) {
  let lo = 0;
  let hi = Math.max(0, offsets.length - 2);
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (offsets[mid + 1] <= y) lo = mid + 1;
    else hi = mid;
  }
  return lo;
}

/**
 * The window `[first, last)` of fixed-height rows.
 * @param {{ scrollTop: number, viewHeight: number, rowHeight: number, count: number, overscan: number }} at
 */
export function windowOf({ scrollTop, viewHeight, rowHeight, count, overscan }) {
  if (!(viewHeight > 0) || !(rowHeight > 0) || count <= 0) return { first: 0, last: 0 };
  const top = Math.max(0, scrollTop);
  const first = Math.max(0, Math.floor(top / rowHeight) - overscan);
  const last = Math.min(count, Math.ceil((top + viewHeight) / rowHeight) + overscan);
  return { first, last: Math.max(first, last) };
}

/**
 * The window over measured offsets.
 * @param {{ offsets: readonly number[], scrollTop: number, viewHeight: number, overscan: number }} at
 */
export function windowAt({ offsets, scrollTop, viewHeight, overscan }) {
  const count = offsets.length - 1;
  if (!(viewHeight > 0) || count <= 0) return { first: 0, last: 0 };
  const top = Math.max(0, scrollTop);
  const first = Math.max(0, indexAt(offsets, top) - overscan);
  const last = Math.min(count, indexAt(offsets, top + viewHeight) + 1 + overscan);
  return { first, last: Math.max(first, last) };
}

/** The spacer that stands for every row: `count` rows of `rowHeight`. */
export function spacerHeight(count, rowHeight) {
  return Math.max(0, count) * Math.max(0, rowHeight);
}

/**
 * `scrollTop` kept inside a total that may have shrunk: at most the last
 * viewport's worth, never below zero.
 * @param {number} scrollTop
 * @param {number} total
 * @param {number} viewHeight
 */
export function clampScroll(scrollTop, total, viewHeight) {
  const most = Math.max(0, total - Math.max(0, viewHeight));
  return Math.min(Math.max(0, scrollTop), most);
}

/**
 * An unbounded list's own frame inside the panel that scrolls it: the
 * panel's scroll, less where the list starts, clamped to the list; the
 * viewport is the panel's — rows above or below the panel's view are
 * outside the window whatever the list's height.
 * @param {{ scrollTop: number, viewHeight: number, offsetTop: number, total: number }} panel
 */
export function localFrame({ scrollTop, viewHeight, offsetTop, total }) {
  const local = Math.min(Math.max(0, scrollTop - offsetTop), Math.max(0, total));
  return { scrollTop: local, viewHeight: Math.max(0, viewHeight) };
}
