/**
 * Where a match falls among the text nodes of a rendering. The walker in
 * `useDomFind.ts` hands over the nodes' texts in document order; this maps
 * an offset in their concatenation back to the node and the offset within
 * it, so a match that crosses two nodes becomes two ranges. Pure, so
 * `node --test` reads it; the DOM is the hook's.
 */

/**
 * The cumulative starts of each text, and the whole.
 * @param {readonly string[]} texts
 * @returns {{starts: number[], text: string}}
 */
export function segmentsOf(texts) {
  const starts = [];
  let at = 0;
  for (const t of texts) {
    starts.push(at);
    at += t.length;
  }
  return { starts, text: texts.join("") };
}

/**
 * Where a scrollport should stand for a match to be seen: `null` when the
 * match is already wholly inside it; else the scroll top that centres the
 * match — or, for one taller than the port, aligns its top. The rects are
 * as `getBoundingClientRect` gives them, the port's being its client box;
 * a negative answer is clamped to the top, the far end is the browser's to
 * clamp.
 * @param {{top: number, bottom: number}} match the match's rect
 * @param {{top: number, bottom: number}} port the scrollport's client rect
 * @param {number} scrollTop the port's scroll top now
 * @returns {number | null}
 */
export function revealOffset(match, port, scrollTop) {
  const height = port.bottom - port.top;
  const tall = match.bottom - match.top;
  if (match.top >= port.top && match.bottom <= port.bottom) return null;
  const above = match.top - port.top;
  const centred = tall >= height ? above : above - (height - tall) / 2;
  return Math.max(0, scrollTop + centred);
}

/**
 * The spans of one match over the nodes: `{node, start, end}` per node the
 * match touches, offsets within that node. An empty match is no span.
 * @param {{starts: number[]}} segments
 * @param {readonly string[]} texts
 * @param {{start: number, end: number}} match
 * @returns {{node: number, start: number, end: number}[]}
 */
export function locate(segments, texts, match) {
  const out = [];
  if (match.end <= match.start) return out;
  for (let i = 0; i < segments.starts.length; i++) {
    const from = segments.starts[i];
    const to = from + texts[i].length;
    if (to <= match.start) continue;
    if (from >= match.end) break;
    out.push({ node: i, start: Math.max(match.start, from) - from, end: Math.min(match.end, to) - from });
  }
  return out;
}
