/**
 * Bounded undo and redo over a value, with no React in it.
 *
 * The designer's history is a list of definitions: every edit pushes one,
 * undo walks back, redo walks forward, and a new edit after an undo drops the
 * future the way every editor does. Bounded because a definition is not
 * large but an afternoon of dragging is.
 *
 * Two things ride with the list so an undo lands where the person was:
 *
 * - **A selection per entry.** Every entry that leaves the head is stamped
 *   with the selection the person had while it was the head, and undo or
 *   redo hands that selection back as `landed`: undoing a delete selects the
 *   step again, undoing an add selects what was selected before it. The
 *   history never reads the selection — it is whatever shape the caller
 *   keeps (`selectionModel.mjs`'s), stored and handed back as given.
 * - **Coalescing.** A typed burst is one undo step, not one per keystroke:
 *   a push that carries the same `token` as the head's, within
 *   `COALESCE_MS` of it, replaces the head instead of pushing it down. A
 *   token names what is being typed into (`step:<id>`); a structural edit
 *   carries none and never coalesces, so a drag typed right after a word
 *   is its own step, and so is the word.
 *
 * `present` stays the bare value: the goal tab's draft store persists it
 * and nothing else.
 */

const DEFAULT_CAPACITY = 100;

/** How long after the last push a push with the same token still merges into it, in ms. */
export const COALESCE_MS = 800;

export function create(initial, capacity = DEFAULT_CAPACITY) {
  return { past: [], present: initial, future: [], capacity, at: 0, token: null, landed: null };
}

/**
 * Record a new present. A value equal by reference is not a new edit. With a
 * `token` equal to the head's and `at` within the window, the head is replaced
 * — the burst stays one step; otherwise the head goes to the past, stamped with
 * `selection`, and the future is dropped.
 * @param {{past: object[], present: unknown, future: object[], capacity: number, at: number, token: string | null}} h
 * @param {unknown} next
 * @param {{selection?: unknown, at?: number, token?: string | null}} [edit]
 */
export function push(h, next, { selection = null, at = 0, token = null } = {}) {
  if (next === h.present) return h;
  if (token !== null && token === h.token && at - h.at <= COALESCE_MS && at >= h.at) {
    return { ...h, present: next, at, landed: null };
  }
  const past = [...h.past, { value: h.present, selection }];
  if (past.length > h.capacity) past.shift();
  return { ...h, past, present: next, future: [], at, token, landed: null };
}

/**
 * Back one step. The head goes to the future stamped with `selection` — where
 * the person is now, for redo to come back to — and `landed` is the selection
 * the restored entry was left with. A restored head never coalesces.
 * @param {object} h
 * @param {unknown} [selection]
 */
export function undo(h, selection = null) {
  if (h.past.length === 0) return h;
  const past = h.past.slice(0, -1);
  const entry = h.past[h.past.length - 1];
  return { ...h, past, present: entry.value, future: [{ value: h.present, selection }, ...h.future], at: 0, token: null, landed: entry.selection };
}

/** Forward one step, the mirror of `undo`. */
export function redo(h, selection = null) {
  if (h.future.length === 0) return h;
  const [entry, ...future] = h.future;
  return { ...h, past: [...h.past, { value: h.present, selection }], present: entry.value, future, at: 0, token: null, landed: entry.selection };
}

export const canUndo = (h) => h.past.length > 0;
export const canRedo = (h) => h.future.length > 0;
