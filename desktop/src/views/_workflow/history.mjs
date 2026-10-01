/**
 * Bounded undo and redo over a value, with no React in it.
 *
 * The designer's history is a list of definitions: every edit pushes one,
 * undo walks back, redo walks forward, and a new edit after an undo drops the
 * future the way every editor does. Bounded because a definition is not
 * large but an afternoon of dragging is.
 */

const DEFAULT_CAPACITY = 100;

export function create(initial, capacity = DEFAULT_CAPACITY) {
  return { past: [], present: initial, future: [], capacity };
}

/** Record a new present. A value equal by reference is not a new edit. */
export function push(h, next) {
  if (next === h.present) return h;
  const past = [...h.past, h.present];
  if (past.length > h.capacity) past.shift();
  return { ...h, past, present: next, future: [] };
}

export function undo(h) {
  if (h.past.length === 0) return h;
  const past = h.past.slice(0, -1);
  return { ...h, past, present: h.past[h.past.length - 1], future: [h.present, ...h.future] };
}

export function redo(h) {
  if (h.future.length === 0) return h;
  const [next, ...future] = h.future;
  return { ...h, past: [...h.past, h.present], present: next, future };
}

export const canUndo = (h) => h.past.length > 0;
export const canRedo = (h) => h.future.length > 0;
