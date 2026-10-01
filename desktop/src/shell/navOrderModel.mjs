/**
 * The order of the sidebar's destinations — the person's, not the file's.
 *
 * `nav.ts` says *which* destinations there are; this says what order they
 * stand in once someone has dragged one (guide/the-desktop.md §Sidebar).
 * The order is a list of keys kept per viewer (`navOrderStore.ts`, in
 * `localStorage` under `NAV_ORDER_KEY`), and it is made whole here against
 * the destinations that exist: a stored key that is no destination is
 * dropped, a destination that was never stored is appended in the default
 * order, a repeat counts once — so a new destination shows up and an old
 * one goes without anybody's order breaking. Nothing stored, or garbage, is
 * the default. Pure, so `node --test` reads it.
 */

import { moveIndex } from "../ui/dnd/sortModel.mjs";

/** Where the order is kept, per viewer. */
export const NAV_ORDER_KEY = "bisa.sidebar.order";

/**
 * The person's order made whole against the destinations that exist.
 * @param {unknown} stored what `localStorage` held, parsed — or null
 * @param {readonly string[]} defaults the destinations' keys in the file's order
 * @returns {string[]}
 */
export function orderKeys(stored, defaults) {
  const known = new Set(defaults);
  const out = [];
  if (Array.isArray(stored)) {
    for (const k of stored) {
      if (typeof k === "string" && known.has(k) && !out.includes(k)) out.push(k);
    }
  }
  for (const k of defaults) if (!out.includes(k)) out.push(k);
  return out;
}

/**
 * The order with `key` at `index` — clamped to the ends; the same array when
 * nothing moves or the key is not in it.
 * @param {readonly string[]} order
 * @param {string} key
 * @param {number} index
 */
export function placeKey(order, key, index) {
  return moveIndex(order, order.indexOf(key), index);
}

/**
 * The entries in the given order. An entry the order does not name keeps
 * its place after the named ones, in the entries' own order.
 * @template {{key: string}} E
 * @param {readonly E[]} nav
 * @param {readonly string[]} order
 * @returns {E[]}
 */
export function orderedNav(nav, order) {
  const at = new Map(order.map((k, i) => [k, i]));
  return [...nav]
    .map((e, i) => ({ e, rank: at.has(e.key) ? at.get(e.key) : order.length + i }))
    .sort((a, b) => a.rank - b.rank)
    .map(({ e }) => e);
}

/**
 * The destination the app opens on: the first of the person's order, the
 * file's first when nothing is ordered yet. A launch, a bare hash and a hash
 * that names nothing all land here (`router.ts` asks `navOrderStore.homeRoute`).
 * @param {readonly string[]} order
 * @param {readonly string[]} defaults
 */
export function homeKey(order, defaults) {
  return order[0] ?? defaults[0];
}

/**
 * Whether the order is the file's.
 * @param {readonly string[]} order
 * @param {readonly string[]} defaults
 */
export function isDefaultOrder(order, defaults) {
  return order.length === defaults.length && order.every((k, i) => k === defaults[i]);
}
