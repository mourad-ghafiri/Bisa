/**
 * The settings store's rules, without React: how a read is keyed, and how
 * a read that answers settles into the entry it refreshes — keeping the
 * list's identity when nothing changed, so a control memoised on the
 * resolved values re-renders only when a value did.
 */

import { sameJsonList } from "./snapshotEqual.mjs";

/** The key a resolved read is kept under: the workspace's, or a project's. */
export function keyOf(project) {
  return project ? String(project) : "workspace";
}

/** An entry nothing has read yet. */
export const EMPTY_ENTRY = Object.freeze({ data: null, error: null, loading: false, refreshing: false, at: null });

/** The entry while its read is in flight: the first read is `loading`, a later one `refreshing` over the last answer. */
export function reading(entry) {
  const has = entry.data !== null;
  return { ...entry, loading: !has, refreshing: has };
}

/**
 * The entry once its read answered `list` at `at`: the last answer stands
 * when the lists are the same by value.
 * @param {{data: readonly unknown[] | null}} entry
 * @param {readonly unknown[]} list
 * @param {number} at unix seconds
 */
export function settled(entry, list, at) {
  const data = entry.data !== null && sameJsonList(entry.data, list) ? entry.data : list;
  return { data, error: null, loading: false, refreshing: false, at };
}

/** The entry once its read refused: the last answer stands, the reason beside it. */
export function refused(entry, error) {
  return { ...entry, error: String(error), loading: false, refreshing: false };
}
