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
