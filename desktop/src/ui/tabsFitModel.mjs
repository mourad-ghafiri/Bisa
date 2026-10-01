/**
 * How a tab strip folds when its words would not fit — in stages, in an
 * order the tabs declare. A tab may say the turn in which it yields its
 * word (`fold`, lower first); tabs that say none yield together, first; a
 * tab with no glyph never folds, since it has nothing to fold to. Stage 0
 * is every word drawn; stage i is the first i groups folded to their
 * glyphs. The strip is measured at every stage it draws — `needed[i]` is
 * the width it took there — and `available` is the width its container
 * gives it now: the stage drawn is the first that fits, a stage not yet
 * measured being drawn to be measured, and the last when none fits. Room
 * grows: the walk starts from 0 again over the remembered widths, so the
 * words come back in the reverse of the order they went. Nothing is
 * scrolled off, wrapped or hidden: every tab stays visible and reachable
 * on one row at every width. Plain `.mjs`, so `node --test` reads it.
 */

/**
 * The tabs that yield their word together, group by group in yielding
 * order: ranks ascending, tabs without a rank first, ids in strip order
 * within a group. A tab with no glyph is in no group.
 * @param {readonly {id: string, icon?: unknown, fold?: number}[]} tabs
 * @returns {string[][]}
 */
export function foldGroups(tabs) {
  const byRank = new Map();
  for (const t of tabs ?? []) {
    if (t.icon === undefined) continue;
    const rank = typeof t.fold === "number" && Number.isFinite(t.fold) ? t.fold : Number.NEGATIVE_INFINITY;
    if (!byRank.has(rank)) byRank.set(rank, []);
    byRank.get(rank).push(t.id);
  }
  return [...byRank.entries()].sort((a, b) => a[0] - b[0]).map(([, ids]) => ids);
}

/**
 * The stage the strip draws: the first, from 0, that is not yet measured
 * (draw it to measure it) or that fits; the last when none fits; 0 when
 * the container's width is not known.
 * @param {readonly (number | null | undefined)[]} needed the strip's width at each stage it has drawn
 * @param {number | null | undefined} available the container's width, when known
 * @param {number} stages how many groups there are — the last stage folds them all
 * @returns {number}
 */
export function fitStage(needed, available, stages) {
  if (stages <= 0) return 0;
  if (available == null || !Number.isFinite(available)) return 0;
  for (let stage = 0; stage <= stages; stage += 1) {
    const w = needed?.[stage];
    if (w == null || !Number.isFinite(w)) return stage;
    if (w <= available) return stage;
  }
  return stages;
}

/**
 * The ids folded at a stage: the first `stage` groups.
 * @param {readonly (readonly string[])[]} groups
 * @param {number} stage
 * @returns {Set<string>}
 */
export function foldedIds(groups, stage) {
  return new Set(groups.slice(0, Math.max(0, stage)).flat());
}

/**
 * What the measured widths are keyed by: a change to a tab's id, label,
 * glyph, fold rank or count is a change of width, and forgets them all.
 * @param {readonly {id: string, label: string, icon?: unknown, fold?: number, count?: number | null}[]} tabs
 * @returns {string}
 */
export function stripKey(tabs) {
  return (tabs ?? []).map((t) => `${t.id}${t.label}${t.icon === undefined ? "" : "g"}${t.fold ?? ""}${t.count ?? ""}`).join("");
}

/**
 * The tooltip a folded tab wears: its label, and its count when it has one.
 * @param {string} label
 * @param {number | null | undefined} count
 * @returns {string}
 */
export function glyphTitle(label, count) {
  return typeof count === "number" && count > 0 ? `${label} · ${count}` : label;
}
