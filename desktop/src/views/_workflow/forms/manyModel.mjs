/**
 * A field that takes one value or a list (`OneOrMany` on the wire), as the
 * forms hold one. A list of one is written as the bare value — a design's
 * digest moves only when the author's words do — and an empty list as
 * nothing, which the wire reads as absent: any, or the field's own default.
 * `asList` reads the other way (`startForm.mjs`). Every event filter's form
 * draws its lists through these, so a word is added, removed or toggled in
 * one place.
 */

import { asList } from "./startForm.mjs";

/** Two entries the same: a word by its letters, a reference by its shape. */
function same(a, b) {
  return a === b || (typeof a === "object" && typeof b === "object" && JSON.stringify(a) === JSON.stringify(b));
}

/** A list as the wire takes it: nothing for none, the bare value for one, the list otherwise. */
export function fromList(list) {
  const items = (list ?? []).filter((v) => v !== null && v !== undefined && v !== "");
  if (items.length === 0) return null;
  return items.length === 1 ? items[0] : items;
}

/** Whether a one-or-many value holds `word`. */
export function hasWord(v, word) {
  return asList(v).some((x) => same(x, word));
}

/** The value with `word` added when absent, removed when present. */
export function toggleWord(v, word) {
  const list = asList(v);
  return fromList(list.some((x) => same(x, word)) ? list.filter((x) => !same(x, word)) : [...list, word]);
}

/** The value with `word` added once — a string trimmed; an empty word, or one already there, adds nothing. */
export function addWord(v, word) {
  const w = typeof word === "string" ? word.trim() : word;
  if (w === "" || w === null || w === undefined) return v ?? null;
  const list = asList(v);
  return list.some((x) => same(x, w)) ? (v ?? null) : fromList([...list, w]);
}

/** The value without `word`. */
export function removeWord(v, word) {
  return fromList(asList(v).filter((x) => !same(x, word)));
}

/** The words a closed-set field holds, or `fallback` when it holds none — what the wire reads an absent field as; `[]` for a fallback of none. */
export function wordsOr(v, fallback) {
  const list = asList(v);
  return list.length > 0 ? list : asList(fallback);
}

/** A list split into the words of a closed set and the rest — a message's `from`, where the words and the named senders share one list. */
export function splitWords(v, words) {
  const list = asList(v);
  const isWord = (x) => typeof x === "string" && words.includes(x);
  return { words: list.filter(isWord), rest: list.filter((x) => !isWord(x)) };
}

/** What such a field writes for `words` beside `rest`: nothing when both are empty. */
export function joinWords(words, rest) {
  return fromList([...asList(words), ...asList(rest)]);
}

/**
 * The suggestions a words input offers beside its line: the ones not yet
 * chosen — all of them, or, for a list too long to show as chips (`typed`),
 * none until the line holds something, then the ones that contain it,
 * case-folded, at most `cap`.
 * @template {{ word: string }} S
 * @param {readonly S[]} suggestions
 * @param {readonly string[]} chosen
 * @param {string} draft what the line holds
 * @param {{ typed?: boolean, cap?: number }} [how]
 * @returns {S[]}
 */
export function offeredSuggestions(suggestions, chosen, draft, { typed = false, cap = 8 } = {}) {
  const left = (suggestions ?? []).filter((s) => !(chosen ?? []).includes(s.word));
  if (!typed) return left;
  const needle = (draft ?? "").trim().toLowerCase();
  if (!needle) return [];
  return left.filter((s) => s.word.toLowerCase().includes(needle)).slice(0, cap);
}
