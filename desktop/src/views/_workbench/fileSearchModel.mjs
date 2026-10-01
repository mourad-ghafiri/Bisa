/**
 * The Files tab's search (ide/03), as facts: what a typed query
 * asks for, how name hits are ranked, how content hits fold into files, and
 * what the footer says. Names are matched here, over the path index the
 * quick-open palette already keeps; contents come from the node's ripgrep
 * stream (`GET /ide/search`) and are only grouped here. Nothing in this file
 * touches the network.
 */

import { rankPaths } from "../../shell/quickOpenScore.mjs";
import { t } from "../../i18n/l10n.mjs";

/** How many hits one file shows before it folds the rest into "+N more". */
const HITS_PER_FILE = 20;
/** How many name hits to show. */
const NAME_CAP = 200;

/**
 * Parse the box: the words to search for and the glob tokens that narrow
 * where. `in:src/**` includes, `-in:*.lock` excludes; everything else is the
 * needle, whitespace-joined.
 * @param {string} text
 * @returns {{needle: string, include: string[], exclude: string[]}}
 */
export function parseQuery(text) {
  const include = [];
  const exclude = [];
  const words = [];
  for (const tok of String(text ?? "").trim().split(/\s+/)) {
    if (!tok) continue;
    if (tok.startsWith("-in:") && tok.length > 4) exclude.push(tok.slice(4));
    else if (tok.startsWith("in:") && tok.length > 3) include.push(tok.slice(3));
    else words.push(tok);
  }
  return { needle: words.join(" "), include, exclude };
}

/**
 * Files whose path matches the needle, best first — the palette's scorer,
 * so the same query finds the same files here and there.
 * @param {readonly string[]} index
 * @param {string} needle
 * @param {number} [cap]
 * @returns {string[]}
 */
export function nameResults(index, needle, cap = NAME_CAP) {
  if (!needle.trim()) return [];
  return rankPaths(needle, index, cap).map((r) => r.path);
}

/**
 * Content hits folded into their files, in the order the files first
 * appeared — ripgrep's walk order, which is stable across a re-run. Each
 * file keeps its first `perFile` hits and counts the rest.
 * @param {readonly {path: string, line: number, column: number, text: string}[]} hits
 * @param {number} [perFile]
 * @returns {{path: string, hits: {line: number, column: number, text: string}[], more: number}[]}
 */
export function groupHits(hits, perFile = HITS_PER_FILE) {
  const byPath = new Map();
  const out = [];
  for (const h of hits) {
    let g = byPath.get(h.path);
    if (!g) {
      g = { path: h.path, hits: [], more: 0 };
      byPath.set(h.path, g);
      out.push(g);
    }
    if (g.hits.length < perFile) g.hits.push({ line: h.line, column: h.column, text: h.text });
    else g.more += 1;
  }
  return out;
}

/**
 * The footer's sentence. `summary` is the node's `done` frame, or null while
 * the search runs; `live` says a stream is still open.
 * @param {{matches: number, files_with_matches: number, files_scanned: number, truncated: boolean} | null} summary
 * @param {boolean} live
 * @param {number} shown hits received so far
 */
export function searchStatus(summary, live, shown) {
  if (live) return shown === 0 ? "searching…" : t("workbench-file-search-so-far", { shown });
  if (!summary) return "";
  const n = summary.matches;
  const files = summary.files_with_matches;
  const base = n === 0 ? t("workbench-file-search-no-match-files", { files_scanned: summary.files_scanned }) : t("workbench-file-search-match-matches-file-files-scanned", { n, files, files_scanned: summary.files_scanned });
  return summary.truncated ? t("workbench-file-search-stopped-cap-narrow", { base }) : base;
}

/**
 * The flat list a results pane draws: a file row, then its hit rows, then a
 * "+N more" row when it folded some. Flat because the list is virtualised.
 * @param {ReturnType<typeof groupHits>} groups
 * @returns {({kind: "file", key: string, path: string, count: number} | {kind: "hit", key: string, path: string, line: number, column: number, text: string} | {kind: "more", key: string, path: string, more: number})[]}
 */
export function resultRows(groups) {
  const rows = [];
  for (const g of groups) {
    rows.push({ kind: "file", key: `f:${g.path}`, path: g.path, count: g.hits.length + g.more });
    for (const h of g.hits) rows.push({ kind: "hit", key: `h:${g.path}:${h.line}:${h.column}`, path: g.path, line: h.line, column: h.column, text: h.text });
    if (g.more > 0) rows.push({ kind: "more", key: `m:${g.path}`, path: g.path, more: g.more });
  }
  return rows;
}

/**
 * The next openable row after `cursor` (or before), wrapping — Enter and
 * Shift+Enter in the box. Only `hit` rows (and, for a name search, `file`
 * rows) open; `more` rows are notes. -1 for nothing openable.
 * @param {ReturnType<typeof resultRows>} rows
 * @param {number} cursor
 * @param {1 | -1} dir
 */
export function nextOpenable(rows, cursor, dir) {
  const n = rows.length;
  if (n === 0) return -1;
  const openable = (r) => r.kind !== "more";
  for (let step = 1; step <= n; step++) {
    const i = (((cursor + dir * step) % n) + n) % n;
    if (openable(rows[i])) return i;
  }
  return -1;
}

/** A hit's line with the match itself marked, for a row: `[before, match, after]`. */
export function splitHit(text, column, needleLength) {
  const at = Math.max(0, column - 1);
  const len = Math.max(0, needleLength);
  return [text.slice(0, at), text.slice(at, at + len), text.slice(at + len)];
}
