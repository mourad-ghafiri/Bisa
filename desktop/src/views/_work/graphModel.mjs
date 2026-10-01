/**
 * The commit graph's geometry and search (ide/05), in plain JavaScript so
 * `node --test` runs it without a build.
 *
 * The engine lays lanes out; this file turns one row — and the row above it —
 * into the strokes an SVG cell draws, and answers "which rows match" without
 * touching the topology. Every number here is in *lane units* and *row
 * fractions*: `x = lane * LANE_W + LANE_W / 2`, `y ∈ [0, 1]` of the row, so
 * the renderer scales them by the row height it happens to use.
 */

import { t } from "../../i18n/l10n.mjs";

export const LANE_W = 14;
export const NODE_R = 3.5;

/** Lane colours, from the theme's own roles so they track the accent and the contrast rules. */
const LANE_TOKENS = Object.freeze([
  "var(--color-accent)",
  "var(--color-ok)",
  "var(--color-warn)",
  "var(--color-danger)",
  "var(--color-accent-ink)",
  "var(--color-text-dim)",
]);

/** @param {number} lane */
export function laneColor(lane) {
  return LANE_TOKENS[lane % LANE_TOKENS.length];
}

/** @param {number} lane */
export function laneX(lane) {
  return lane * LANE_W + LANE_W / 2;
}

/**
 * The strokes of one row. `prev` is the row above (its `passing` lanes enter
 * this row from the top), `null` for the first row.
 *
 * @param {import("../../types").GraphRow} row
 * @param {import("../../types").GraphRow | null} prev
 * @returns {import("./graphModel.d.mts").Stroke[]}
 */
export function strokesFor(row, prev) {
  /** @type {import("./graphModel.d.mts").Stroke[]} */
  const out = [];
  const entering = new Set(prev ? prev.passing : []);
  const leaving = new Set(row.passing);
  const merged = new Set(row.edges.filter((e) => e.kind === "merge").map((e) => e.from));
  const forked = new Set(row.edges.filter((e) => e.kind === "fork").map((e) => e.to));

  // Lines entering from the top: into the node if they end here, straight
  // through if they keep going, a curve into the node for a merge.
  for (const lane of entering) {
    if (lane === row.lane) {
      out.push({ kind: "line", lane, x1: laneX(lane), y1: 0, x2: laneX(lane), y2: 0.5 });
    } else if (merged.has(lane)) {
      out.push({ kind: "curve", lane, x1: laneX(lane), y1: 0, x2: laneX(row.lane), y2: 0.5 });
    } else if (leaving.has(lane)) {
      out.push({ kind: "line", lane, x1: laneX(lane), y1: 0, x2: laneX(lane), y2: 0.5 });
    }
  }
  // Lines leaving through the bottom: the node's own lane if it has a
  // parent, forks curving out of the node, everything else straight.
  for (const lane of leaving) {
    if (lane === row.lane) {
      out.push({ kind: "line", lane, x1: laneX(lane), y1: 0.5, x2: laneX(lane), y2: 1 });
    } else if (forked.has(lane)) {
      out.push({ kind: "curve", lane, x1: laneX(row.lane), y1: 0.5, x2: laneX(lane), y2: 1 });
    } else {
      out.push({ kind: "line", lane, x1: laneX(lane), y1: 0.5, x2: laneX(lane), y2: 1 });
    }
  }
  return out;
}

/**
 * How wide the lane column has to be for this window of rows: the widest
 * lane anywhere in it, plus one for the node.
 * @param {import("../../types").GraphRow[]} rows
 */
export function lanesWidth(rows) {
  let max = 0;
  for (const r of rows) {
    if (r.lane > max) max = r.lane;
    for (const p of r.passing) if (p > max) max = p;
  }
  return (max + 1) * LANE_W;
}

/**
 * Does a row match a search? Author, subject, id and refs, case-insensitively
 * — the same rule the node's `graph::row_matches` applies, so a row this
 * highlights is a row the search found. An empty query matches everything,
 * so nothing dims.
 * @param {import("../../types").GraphRow} row
 * @param {string} query
 */
export function matches(row, query) {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  if (row.subject.toLowerCase().includes(q)) return true;
  if (row.author.toLowerCase().includes(q)) return true;
  if (row.id.toLowerCase().startsWith(q) || row.short.toLowerCase().startsWith(q)) return true;
  return row.refs.some((r) => r.name.toLowerCase().includes(q));
}

/** Rows a window asks for at a time; a hole is filled in pages of this. */
export const PAGE = 400;

/**
 * The rows of a graph as the client holds them: an array of `total` slots,
 * a row where a window has landed and `undefined` where none has — so a
 * jump to row 87,000 is one window away, not two hundred pages of scrolling.
 * @param {number} total
 * @returns {(import("../../types").GraphRow | undefined)[]}
 */
export function emptyRows(total) {
  return new Array(Math.max(0, total));
}

/**
 * Lay a window into the sparse rows. A `total` that changed (the layout
 * grew, or the repository moved on) resizes the array, keeping what still
 * fits. Returns the same array when nothing changed.
 * @param {(import("../../types").GraphRow | undefined)[]} rows
 * @param {import("../../types").GraphWindow} w
 */
export function mergeWindow(rows, w) {
  let out = rows;
  if (rows.length !== w.total) {
    out = emptyRows(w.total);
    for (let i = 0; i < Math.min(rows.length, w.total); i++) out[i] = rows[i];
  }
  let changed = out !== rows;
  for (let i = 0; i < w.rows.length; i++) {
    const at = w.from + i;
    if (at >= out.length) break;
    if (out[at] !== w.rows[i] && (out[at]?.id !== w.rows[i].id || out[at] === undefined)) {
      if (out === rows) out = [...rows];
      out[at] = w.rows[i];
      changed = true;
    }
  }
  return changed ? out : rows;
}

/**
 * The windows to fetch so that rows `first..last` (exclusive) are all
 * present: page-aligned `{from, count}` pairs, only for pages with a hole.
 * @param {(import("../../types").GraphRow | undefined)[]} rows
 * @param {number} first
 * @param {number} last
 * @param {number} [page]
 * @returns {{from: number, count: number}[]}
 */
export function holesIn(rows, first, last, page = PAGE) {
  const out = [];
  const lo = Math.max(0, first);
  const hi = Math.min(rows.length, last);
  for (let start = Math.floor(lo / page) * page; start < hi; start += page) {
    const end = Math.min(start + page, rows.length);
    let hole = false;
    for (let i = Math.max(start, lo); i < Math.min(end, hi); i++) {
      if (rows[i] === undefined) {
        hole = true;
        break;
      }
    }
    if (hole) out.push({ from: start, count: end - start });
  }
  return out;
}

/**
 * The next index in an ascending list of match indexes after `cursor` (or
 * before, `dir = -1`), wrapping around; `-1` for no matches. The cursor may
 * be a row that is not itself a match.
 * @param {readonly number[]} indices
 * @param {number} cursor
 * @param {1 | -1} dir
 */
export function nextIndex(indices, cursor, dir) {
  const n = indices.length;
  if (n === 0) return -1;
  if (dir === 1) {
    const i = indices.findIndex((x) => x > cursor);
    return i === -1 ? indices[0] : indices[i];
  }
  for (let i = n - 1; i >= 0; i--) if (indices[i] < cursor) return indices[i];
  return indices[n - 1];
}

/**
 * The header's words for a search: how many matched, where the cursor is,
 * and whether the whole log was there to search.
 * @param {{indices: number[], searched: number, done: boolean, truncated: boolean} | null} m
 * @param {number} cursor
 */
export function searchStatus(m, cursor) {
  if (!m) return "";
  const n = m.indices.length;
  if (n === 0) return m.done ? t("work-graph-no-match") : t("work-graph-no-match-first-rows-still-laying", { searched: m.searched });
  const at = m.indices.indexOf(cursor);
  const count = m.truncated ? `${n}+` : `${n}`;
  const where = at === -1 ? t("work-graph-match-matches", { count, flag: (n === 1 && !m.truncated) ? "yes" : "no" }) : t("work-graph-words", { at: at + 1, count });
  return m.done ? where : t("work-graph-first-rows-still-laying-out", { where, searched: m.searched });
}

/**
 * The header's stable count — *1,234 commits*, or *1,234 so far* while the
 * tail is still being laid out — so the number never hides inside a sentence.
 * @param {{total: number, done: boolean, stale: boolean}} w
 */
export function commitCount(w) {
  const n = w.total.toLocaleString("en-US");
  return w.done && !w.stale ? t("work-graph-commit-commits", { n, total: w.total }) : t("work-graph-so-far", { n });
}

/**
 * One status word beside the count while something is happening — a
 * relayout after the repository moved on, or the first layout still running —
 * and nothing when the graph is settled.
 * @param {{total: number, done: boolean, stale: boolean}} w
 */
export function layoutWord(w) {
  if (w.stale) return t("work-graph-repository-moved-relaying-out");
  if (!w.done) return t("work-graph-laying-out");
  return null;
}

/** The graph's one filter, in the menu's order — the node's `RefScope`. */
export const REF_SCOPES = Object.freeze(["all", "head"]);

/**
 * A ref scope's words: the menu item and what it shows.
 * @param {"all" | "head"} scope
 */
export function refScopeWords(scope) {
  return scope === "head"
    ? { label: t("work-graph-show-only-current-branch"), hint: t("work-graph-what-head-reaches-branch-nobody-merged") }
    : { label: t("work-graph-show-all-branches-tags"), hint: t("work-graph-every-local-branch-tag-whole-log") };
}

/**
 * The History view's `⋮` (ide/05): the search, the one filter with the
 * scope in force marked, and *Lay out again* under a rule. Pure — the view
 * binds each id to its act.
 * @param {{refs: "all" | "head", searching: boolean}} facts
 * @returns {{id: string, label: string, icon: string | null, separatorBefore?: boolean, active?: boolean}[]}
 */
/**
 * The branch HEAD is on — the one *check out here* and *switch to this branch*
 * are pointless for — from the loaded row HEAD names; none while detached or
 * before that row is loaded.
 * @param {string | null} headId
 * @param {readonly {id: string, refs: readonly {kind: string, name: string}[]}[]} loaded
 */
export function currentBranchOf(headId, loaded) {
  const head = headId ? loaded.find((r) => r.id === headId) : null;
  return head?.refs.find((r) => r.kind === "branch")?.name ?? null;
}

/**
 * Where the list's cursor goes on a navigation key, clamped to the rows;
 * `null` for a key that is not a move.
 * @param {string} key the event's key
 * @param {number} cursor
 * @param {number} total
 */
export function cursorAfterKey(key, cursor, total) {
  if (total <= 0) return null;
  const clamp = (i) => Math.max(0, Math.min(total - 1, i));
  switch (key) {
    case "ArrowDown":
      return clamp(cursor + 1);
    case "ArrowUp":
      return clamp(cursor - 1);
    case "Home":
      return 0;
    case "End":
      return total - 1;
    default:
      return null;
  }
}

export function historyMenu({ refs, searching }) {
  const items = [{ id: "search", label: searching ? t("work-commit-graph-hide-search") : t("work-graph-search"), icon: "search" }];
  for (const scope of REF_SCOPES) {
    items.push({
      id: `refs:${scope}`,
      label: refScopeWords(scope).label,
      icon: scope === refs ? "check" : null,
      active: scope === refs,
      separatorBefore: scope === REF_SCOPES[0],
    });
  }
  items.push({ id: "relayout", label: t("work-graph-lay-out-again"), icon: "refresh", separatorBefore: true });
  return items;
}
