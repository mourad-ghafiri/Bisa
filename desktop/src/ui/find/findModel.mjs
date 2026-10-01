/**
 * Find, and find-and-replace, as one fact for every surface that reads a
 * document: the editor's widget opens on the app's chord, a rendering
 * highlights what this finds, a terminal's buffer is searched with the
 * same query shape, and a replacement lands in the buffer through the
 * same two functions. One model, so *regex* and *match case* mean the same
 * thing in every bar and a query that is not a regular expression is
 * refused the same way everywhere — never a throw.
 *
 * Plain `.mjs` with a `.d.mts` beside it, so `node --test` reads it.
 */

/** A query with nothing typed. */
export function emptyFind() {
  return { query: "", regex: false, caseSensitive: false, replacement: "" };
}

/** Whether `find` names something to look for. */
export function hasQuery(find) {
  return typeof find?.query === "string" && find.query.length > 0;
}

/**
 * The global expression a find compiles to: the query itself under
 * `regex`, else the query escaped; `i` unless the case matters. `null` for
 * an empty query and for a regular expression the engine refuses, so a
 * half-typed `[` finds nothing rather than breaking the bar.
 * @param {{query: string, regex: boolean, caseSensitive: boolean}} find
 * @returns {RegExp | null}
 */
export function compileFind(find) {
  if (!hasQuery(find)) return null;
  const source = find.regex ? find.query : find.query.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  try {
    return new RegExp(source, find.caseSensitive ? "g" : "gi");
  } catch {
    return null;
  }
}

/**
 * Every match of `find` in `text`, in order, as `[start, end)` offsets. An
 * expression that matches the empty string advances one character so the
 * loop ends.
 * @param {string} text
 * @returns {{start: number, end: number}[]}
 */
export function matchesOf(text, find) {
  const re = compileFind(find);
  const out = [];
  if (!re) return out;
  const src = String(text ?? "");
  re.lastIndex = 0;
  let m;
  while ((m = re.exec(src)) !== null) {
    out.push({ start: m.index, end: m.index + m[0].length });
    if (m[0].length === 0) re.lastIndex += 1;
  }
  return out;
}

/**
 * The text with every match replaced, and how many were. A regex
 * replacement keeps `$1` and friends; a literal one is literal.
 * @param {string} text
 * @returns {{text: string, count: number}}
 */
export function replaceAll(text, find) {
  const src = String(text ?? "");
  const matches = matchesOf(src, find);
  if (matches.length === 0) return { text: src, count: 0 };
  const re = compileFind(find);
  const replacement = find.regex ? String(find.replacement ?? "") : String(find.replacement ?? "").replace(/\$/g, "$$$$");
  return { text: src.replace(re, replacement), count: matches.length };
}

/**
 * The text with the match at `index` replaced — the one the bar is on —
 * and the same text when there is no such match.
 * @param {string} text
 * @param {number} index
 * @returns {{text: string, replaced: boolean}}
 */
export function replaceOne(text, find, index) {
  const src = String(text ?? "");
  const matches = matchesOf(src, find);
  const hit = matches[index];
  if (!hit) return { text: src, replaced: false };
  const re = compileFind(find);
  const piece = src.slice(hit.start, hit.end);
  const replacement = find.regex ? String(find.replacement ?? "") : String(find.replacement ?? "").replace(/\$/g, "$$$$");
  // Replace the one piece through the same expression, so `$1` reads the
  // groups of this match and nothing before it.
  const single = new RegExp(re.source, re.flags.replace("g", ""));
  const swapped = piece.replace(single, replacement);
  return { text: src.slice(0, hit.start) + swapped + src.slice(hit.end), replaced: true };
}

/**
 * The next index the bar lands on: wrapping past the last match to the
 * first and before the first to the last; `-1` with nothing to land on.
 * @param {number} index the current index, or `-1` for none
 * @param {number} count
 * @param {"next" | "previous"} dir
 */
export function stepIndex(index, count, dir) {
  if (!Number.isInteger(count) || count <= 0) return -1;
  const at = Number.isInteger(index) && index >= 0 && index < count ? index : dir === "next" ? -1 : 0;
  return dir === "next" ? (at + 1) % count : (at - 1 + count) % count;
}

/** The words a bar shows for where it stands: nothing typed, none, or `i/n`. */
export function countWords(index, count) {
  if (count === null || count === undefined) return "";
  if (count === 0) return "none";
  return `${Math.max(0, index) + 1}/${count}`;
}
