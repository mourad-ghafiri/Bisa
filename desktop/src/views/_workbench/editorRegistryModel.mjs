/**
 * The workbench's registry of open editors, as facts: which editor is the
 * active one (a mount claims the slot only until another is focused, and an
 * unmount releases only its own claim), the one line waiting to be shown
 * (asked again for another line the newer wins, asked for another document
 * the older request is dropped, shown at once nothing waits, taken once), and
 * how many things hold unsaved work — what the quit question names.
 * `editorRegistry.ts` holds the handles and the listeners; the rules are here.
 */

/** No editor active, no line waiting. */
export const INITIAL = Object.freeze({ active: null, pending: Object.freeze({ key: null, line: null }) });

/**
 * An editor took focus: it is the active one until another does.
 * @template {{ active: string | null }} S
 * @param {S} state
 * @param {string} key
 * @returns {S}
 */
export function activated(state, key) {
  return state.active === key ? state : { ...state, active: key };
}

/**
 * An editor unmounted: only its own claim is released — a split's other
 * editor keeps the slot it has.
 * @template {{ active: string | null }} S
 * @param {S} state
 * @param {string} key
 * @returns {S}
 */
export function released(state, key) {
  return state.active === key ? { ...state, active: null } : state;
}

/**
 * A line to show — a search hit, a definition, a `:42` — asked for a document.
 * Shown now (`shownNow`), nothing waits; otherwise it is the one request held,
 * replacing whatever waited before, for this document or another: the person
 * has moved on.
 * @template {{ pending: { key: string | null; line: number | null } }} S
 * @param {S} state
 * @param {string} key
 * @param {number} line
 * @param {boolean} shownNow
 * @returns {S}
 */
export function lineRequested(state, key, line, shownNow) {
  return { ...state, pending: shownNow ? INITIAL.pending : { key, line } };
}

/**
 * The line waiting for this document, once: the answer and the state after.
 * Another document's request is left waiting for its own editor.
 * @template {{ pending: { key: string | null; line: number | null } }} S
 * @param {S} state
 * @param {string} key
 * @returns {{ line: number | null; state: S }}
 */
export function lineTaken(state, key) {
  if (state.pending.key !== key || state.pending.line === null) return { line: null, state };
  return { line: state.pending.line, state: { ...state, pending: INITIAL.pending } };
}

/**
 * How many things hold unsaved work: the documents with unsaved buffers and
 * the dirty sources outside the workbench (the designer's draft).
 * @param {readonly string[]} unsavedKeys
 * @param {readonly { dirty: () => boolean }[]} sources
 */
export function dirtyCount(unsavedKeys, sources) {
  return unsavedKeys.length + sources.filter((s) => s.dirty()).length;
}
