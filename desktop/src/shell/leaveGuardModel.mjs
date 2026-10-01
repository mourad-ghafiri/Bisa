/**
 * Leaving a document with unsaved work asks first (ide/03 §Tabs, for the
 * IDE's tabs; here for the Notes and Draw editors): the rules and the words,
 * no React. Which editor holds unsaved work is the editor's fact (its
 * *hold*); this says whether a departure asks, what the question says, and
 * the key the hold is registered under — the same key the quit question
 * counts it by (`editorRegistry.registerDirtySource`).
 *
 * Plain `.mjs`, so `node --test` reads it.
 */

import { t as tr } from "../i18n/l10n.mjs";

/** What a hold answers: whether it is dirty. Only that is read here. */

/**
 * Whether a departure asks: only when something is held and it is dirty.
 * @param {{dirty: () => boolean} | null | undefined} hold
 * @returns {"ask" | "go"}
 */
export function leaveDecision(hold) {
  return hold && hold.dirty() ? "ask" : "go";
}

/**
 * The key a note's or a drawing's hold is registered under — `note:<id>`,
 * `drawing:<id>` — one per record, so a second editor on the same record
 * replaces the first rather than doubling the count.
 * @param {"note" | "drawing"} kind
 * @param {string} id
 */
export function holdKey(kind, id) {
  return `${kind}:${id}`;
}

/**
 * The question, worded for the record: the title asked about, why it asks,
 * and what *Don't save* costs — words typed for a note, strokes drawn for a
 * drawing. An untitled record is named by its kind.
 * @param {"note" | "drawing"} kind
 * @param {string | null | undefined} title
 * @returns {{title: string, description: string, note: string}}
 */
export function leaveWords(kind, title) {
  const name = (title ?? "").trim() || (kind === "note" ? tr("shell-leave-guard-this-note") : tr("shell-leave-guard-this-drawing"));
  return {
    title: tr("shell-leave-guard-save-changes", { title: name }),
    description: tr("shell-leave-guard-since-last-save"),
    note: kind === "note" ? tr("shell-leave-guard-note-typed") : tr("shell-leave-guard-drawing-drew"),
  };
}
