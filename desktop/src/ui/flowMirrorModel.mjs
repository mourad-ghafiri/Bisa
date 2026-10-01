/**
 * The canvas mirror's one rule: **content-addressed, never identity-addressed.**
 *
 * `FlowCanvas` keeps a mirror of the caller's nodes and edges so xyflow's
 * changes (a drag, a selection) have somewhere to land, and resyncs it from
 * the props when the caller changes the picture. A caller re-renders far more
 * often than its picture changes — a goal page refetches on every frame while
 * the Workflow Agent designs — and the arrays it hands over are new objects
 * every time. Resyncing on identity therefore committed a state change on
 * every render and threw away what xyflow had measured, which sent it round
 * again: the loop the Workflow tab used to fall into.
 *
 * `reconcileNodes`/`reconcileEdges` compare what the caller *says* — id,
 * position, type, selection, the renderer's `data` (by reference: the caller
 * memoises it, with `reuse` below) — and hand back the **previous array
 * itself** when nothing changed, which is how React knows there is nothing
 * to commit. When something did change, every node that survives keeps what
 * xyflow learnt about it (`measured`, `width`, `height`), so a resync never
 * costs a re-measure. An edge is its ends, its handles and its look; it
 * carries no route of its own — every flow is a smooth step between its
 * handles.
 */

/** The fields of a node the caller decides; everything else is xyflow's. */
const NODE_FIELDS = Object.freeze(["type", "selected", "draggable", "connectable", "data"]);
/** The fields of an edge the caller decides, beside its bends. */
const EDGE_FIELDS = Object.freeze(["source", "target", "sourceHandle", "targetHandle", "label", "animated", "className", "type", "selected"]);

function samePosition(a, b) {
  return a === b || (!!a && !!b && a.x === b.x && a.y === b.y);
}

function sameFields(a, b, fields) {
  // Absence is absence: a field that is null on one side and missing on the
  // other says the same thing.
  for (const f of fields) if (a[f] !== b[f] && !(a[f] == null && b[f] == null)) return false;
  return true;
}

function sameNode(prev, next) {
  return prev.id === next.id && samePosition(prev.position, next.position) && sameFields(prev, next, NODE_FIELDS);
}

function sameEdge(prev, next) {
  return prev.id === next.id && sameFields(prev, next, EDGE_FIELDS);
}

/**
 * Two records with the same values field for field — the `fields` named, or
 * every own key of either when none are. `null` and `undefined` are the same
 * absence; anything else compares by identity.
 */
export function sameShallow(a, b, fields) {
  if (a === b) return true;
  if (a == null || b == null) return a == null && b == null;
  const keys = fields ?? [...new Set([...Object.keys(a), ...Object.keys(b)])];
  return sameFields(a, b, keys);
}

/**
 * `prev` when `same(prev, next)`, else `next`: how a caller keeps the object
 * it handed over last time when nothing about it changed, so the mirror's
 * by-reference comparison of `data` can hit.
 */
export function reuse(prev, next, same) {
  return prev !== undefined && same(prev, next) ? prev : next;
}

/**
 * The mirror after the caller handed over `next`. Returns `prev` itself when
 * `next` says nothing new; otherwise a new array in `next`'s order, every
 * surviving node keeping xyflow's measurement.
 */
export function reconcileNodes(prev, next) {
  if (prev.length === next.length && next.every((n, i) => sameNode(prev[i], n))) return prev;
  const by = new Map(prev.map((n) => [n.id, n]));
  return next.map((n) => {
    const old = by.get(n.id);
    if (!old) return n;
    const kept = { ...n };
    if (old.measured !== undefined) kept.measured = old.measured;
    if (old.width !== undefined) kept.width = old.width;
    if (old.height !== undefined) kept.height = old.height;
    return kept;
  });
}

/** The same rule for edges: `prev` when nothing changed, else `next`. */
export function reconcileEdges(prev, next) {
  if (prev.length === next.length && next.every((e, i) => sameEdge(prev[i], e))) return prev;
  return next;
}
