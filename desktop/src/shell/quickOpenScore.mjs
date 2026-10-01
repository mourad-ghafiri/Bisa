/**
 * Fuzzy scoring for quick open (ide/12): a query against a path,
 * the way an editor's file switcher does it — every query character must
 * appear in order; runs, word starts and the basename score higher; a shorter
 * path wins a tie. Pure, so the budget in ide/14 (first results under 50 ms
 * on 100k paths) has a test that times it rather than a promise.
 */

import { t } from "../i18n/l10n.mjs";

/** How many rows a section may show before the rest are hidden behind the count. */
export const SECTION_CAPS = Object.freeze({
  Files: 12,
  Commands: 10,
  Projects: 8,
  Workstreams: 8,
  [t("shell-omnibox-work-items")]: 8,
  Agents: 8,
  Conversations: 8,
  Terminals: 6,
});

export const MAX_ROWS = 60;

/** How many rows the palette lists at most, every section counted. */
export const MAX_ITEMS = 40;

/**
 * What the typed words search for: under a prefix mode the words after the
 * prefix, else the whole line — trimmed and folded.
 * @param {string} q the field's text
 * @param {"all" | "places" | "commands" | "line"} mode what the palette is listing
 */
export function paletteNeedle(q, mode) {
  const parsed = parsePrefix(q);
  const prefix = mode === "all" ? null : parsed.prefix;
  return (prefix ? parsed.query : String(q ?? "").trim()).toLowerCase();
}

/**
 * Whether a palette row matches the needle: its label, its hint and its
 * hidden keywords together, folded; an empty needle matches every row.
 * @param {{label: string, hint?: string | null, keywords?: string | null}} item
 * @param {string} needle
 */
export function itemMatches(item, needle) {
  if (!needle) return true;
  return `${item.label} ${item.hint ?? ""} ${item.keywords ?? ""}`.toLowerCase().includes(needle);
}

/**
 * Admit one more row to its section, within the section's cap — so one
 * category cannot flood the list. Counts are the caller's, mutated here.
 * @param {Map<string, number>} counts rows admitted so far, by section
 * @param {string} group the row's section
 */
export function admit(counts, group) {
  const cap = SECTION_CAPS[group] ?? MAX_ROWS;
  const n = counts.get(group) ?? 0;
  if (n >= cap) return false;
  counts.set(group, n + 1);
  return true;
}

/**
 * The rows by section, in the order the sections were first met — the order
 * the rows were built in is the ranking — and the flat list the cursor walks.
 * @template {{group: string}} T
 * @param {readonly T[]} items
 * @returns {{groups: [string, T[]][], flat: T[]}}
 */
export function groupItems(items) {
  const m = new Map();
  for (const it of items) {
    const bucket = m.get(it.group);
    if (bucket) bucket.push(it);
    else m.set(it.group, [it]);
  }
  const groups = [...m.entries()];
  return { groups, flat: groups.flatMap(([, bucket]) => bucket) };
}

const BONUS_BASENAME = 12;
const BONUS_WORD_START = 8;
const BONUS_RUN = 6;
const PENALTY_GAP = 1;

function isSeparator(c) {
  return c === "/" || c === "_" || c === "-" || c === "." || c === " ";
}

/**
 * Score `query` against `path`. `null` when the query does not match;
 * otherwise a number where higher is better. Case-insensitive on the path,
 * the query lower-cased once by the caller.
 */
export function scorePath(queryLower, path) {
  if (!queryLower) return 0;
  const lower = path.toLowerCase();
  const slash = lower.lastIndexOf("/");
  let score = 0;
  let qi = 0;
  let last = -2;
  for (let i = 0; i < lower.length && qi < queryLower.length; i++) {
    if (lower[i] !== queryLower[qi]) continue;
    // Prefer the earliest match that keeps the rest matchable: greedy is
    // fine for a switcher, and it is what keeps this under budget.
    if (i === last + 1) score += BONUS_RUN;
    if (i === 0 || isSeparator(lower[i - 1])) score += BONUS_WORD_START;
    if (i > slash) score += BONUS_BASENAME;
    if (last >= 0 && i > last + 1) score -= Math.min(PENALTY_GAP * (i - last - 1), 10);
    last = i;
    qi++;
  }
  if (qi < queryLower.length) return null;
  // Shorter paths first among equals, and an exact basename match first of all.
  score -= Math.min(lower.length, 200) / 20;
  if (lower.slice(slash + 1) === queryLower) score += 40;
  return score;
}

/**
 * The best `limit` paths for `query`, scored and sorted. Stable on ties by
 * path order, so two runs on one index return one answer.
 */
export function rankPaths(query, paths, limit = SECTION_CAPS.Files) {
  // Spaces are not part of a path anybody types: `mod file 42` means the
  // three fragments in order, as every editor's file switcher reads it.
  const q = query.trim().toLowerCase().replace(/\s+/g, "");
  if (!q) return paths.slice(0, limit).map((path) => ({ path, score: 0 }));
  const hits = [];
  for (const path of paths) {
    const score = scorePath(q, path);
    if (score !== null) hits.push({ path, score });
  }
  hits.sort((a, b) => b.score - a.score || a.path.localeCompare(b.path));
  return hits.slice(0, limit);
}

/**
 * The typed prefix that narrows the palette, and the query after it.
 * `>` commands · `:` a line number in the current file · `#` work items ·
 * `@` symbols (reserved for the language server).
 */
export function parsePrefix(raw) {
  const text = raw.trimStart();
  const first = text[0];
  if (first === ">" || first === ":" || first === "#" || first === "@") {
    return { prefix: first, query: text.slice(1).trim() };
  }
  return { prefix: null, query: text.trim() };
}

/**
 * Whether the index lists a path at all — the node's walk, mirrored
 * (`engine::ide::index`: `.gitignore` honoured, hidden names skipped): not
 * what the root's ignore rules match, which the watcher's frame says, and
 * not a path with a hidden segment (`.git/HEAD`, `.env`, `.cache/x`).
 * @param {string} path root-relative
 * @param {boolean | null | undefined} ignored the frame's word on the path
 */
export function listed(path, ignored) {
  if (ignored === true) return false;
  return !String(path ?? "").split("/").some((segment) => segment.startsWith("."));
}

/**
 * Patch a cached index — a list of **files** — from a `file_changed` frame.
 * Returns the same array when nothing changed, and `null` when the frame
 * says more than a list can take from it, so the caller forgets the index
 * and reads it again:
 *
 * - a path that went takes every path under it, whether it was a file or a
 *   folder — nobody can ask a path that is gone what it was;
 * - a folder renamed moves every file under it; its own name is never a file;
 * - a folder made is no file; one made with things in it — a copy, a tree
 *   dropped in from outside — holds files this list has not heard of: `null`;
 * - a rename from a path this list never held (an ignored file, a folder
 *   that held only ignored files) may have brought files into view: `null`;
 * - **what the node's index leaves out is never put in** (`listed`): a path
 *   the root's ignore rules match — the frame says `ignored` — and a hidden
 *   name. A build writing into `target/` is a thousand frames and no row;
 *   a file renamed out of sight leaves the list.
 * @param {readonly string[]} paths
 * @param {{kind: string, path: string, from?: string | null, dir?: boolean, ignored?: boolean}} event
 * @returns {readonly string[] | null}
 */
export function patchIndex(paths, event) {
  const under = (root) => `${root}/`;
  const without = (gone) => {
    const next = paths.filter((p) => p !== gone && !p.startsWith(under(gone)));
    return next.length === paths.length ? paths : next;
  };
  if (event.kind === "removed") return without(event.path);
  if (event.kind === "created") {
    if (!listed(event.path, event.ignored)) return paths;
    if (event.dir) return paths.some((p) => p.startsWith(under(event.path))) ? paths : null;
    return paths.includes(event.path) ? paths : [...paths, event.path].sort();
  }
  if (event.kind === "renamed") {
    const from = event.from ?? null;
    // Moved out of sight: what the list held under the old name goes, and nothing arrives.
    if (!listed(event.path, event.ignored)) return from === null ? paths : without(from);
    if (from === null) return null;
    const moved = paths.filter((p) => p === from || p.startsWith(under(from)));
    if (moved.length === 0) return null;
    const folder = event.dir === true || moved.some((p) => p !== from);
    const kept = paths.filter((p) => p !== from && !p.startsWith(under(from)));
    const arrived = folder ? moved.filter((p) => p !== from).map((p) => `${event.path}${p.slice(from.length)}`) : [event.path];
    return [...new Set([...kept, ...arrived])].sort();
  }
  return paths;
}
