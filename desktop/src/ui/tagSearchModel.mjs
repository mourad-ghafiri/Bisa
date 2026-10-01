/**
 * Narrowing a list two ways at once — by tags, then by the words typed — as
 * the Agents, Teams, Library and assignee lists all do. One rule, so the four
 * lists cannot drift: the tag filter narrows the roster, the search narrows
 * within whatever it left, and the facets are built from the whole list so a
 * tag you selected cannot vanish from under you as you type. Plain `.mjs`,
 * so `node --test` reads it.
 */

/** No tags selected: everything passes. */
export const NO_TAG_FILTER = Object.freeze({ selected: Object.freeze([]), match: "any" });

/** The most tags a filter read back from a memory selects. */
const MAX_KEPT_TAGS = 64;

/**
 * A tag filter read back from a screen's memory (`shell/viewMemoryStore`):
 * the tags selected, each once, and how they match — or `undefined` for
 * what is no filter, which the screen reads as none selected.
 * @param {unknown} raw
 * @returns {{selected: string[], match: "any" | "all"} | undefined}
 */
export function parseTagFilter(raw) {
  if (!raw || typeof raw !== "object" || Array.isArray(raw) || !Array.isArray(raw.selected)) return undefined;
  const selected = [...new Set(raw.selected.filter((tag) => typeof tag === "string" && tag.length > 0 && tag.length <= 64))].slice(0, MAX_KEPT_TAGS);
  return { selected, match: raw.match === "all" ? "all" : "any" };
}

/**
 * Whether one object's tags pass the filter: none selected passes all; `all`
 * wants every selected tag, `any` one of them.
 * @param {readonly string[] | null | undefined} tags
 * @param {{selected: readonly string[], match: "any" | "all"}} filter
 */
export function passesTagFilter(tags, filter) {
  if (!filter?.selected?.length) return true;
  const have = tags ?? [];
  return filter.match === "all" ? filter.selected.every((t) => have.includes(t)) : filter.selected.some((t) => have.includes(t));
}

/** The words typed, as the needle a haystack is searched for: trimmed and folded. @param {string | null | undefined} query */
export function searchNeedle(query) {
  return String(query ?? "")
    .trim()
    .toLowerCase();
}

/**
 * Whether any of an object's searchable fields carries the needle; an empty
 * needle matches everything, an absent field is skipped.
 * @param {string} needle from `searchNeedle`
 * @param {readonly (string | null | undefined)[]} fields
 */
export function matchesWords(needle, fields) {
  if (!needle) return true;
  return fields.some((f) => typeof f === "string" && f.toLowerCase().includes(needle));
}

/**
 * The items that pass both controls, in the list's order.
 * @template T
 * @param {readonly T[]} items
 * @param {{selected: readonly string[], match: "any" | "all"}} filter
 * @param {string | null | undefined} query
 * @param {(item: T) => readonly string[] | null | undefined} tagsOf
 * @param {(item: T) => readonly (string | null | undefined)[]} fieldsOf
 * @returns {T[]}
 */
export function filterByTagsAndWords(items, filter, query, tagsOf, fieldsOf) {
  const needle = searchNeedle(query);
  return items.filter((item) => passesTagFilter(tagsOf(item), filter) && matchesWords(needle, fieldsOf(item)));
}
