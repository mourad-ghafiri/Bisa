/**
 * What every tree in the app agrees on (ide/03), with no DOM in it: how the
 * arrows move a cursor over a flat list of rows, what typing a letter does,
 * and where a dragged row would land. The file explorer and the project rail
 * are two trees over two different kinds of row; the rules below are the
 * ones they must not disagree about.
 *
 * A row is `{id, depth, label?, focusable?, expandable?, expanded?}` in
 * display order. Depth is the nesting level, and it is enough: a row's
 * parent is the nearest row above it with a smaller depth
 * ({@link parentsFromDepth}), so callers never maintain parent links.
 */

/**
 * Each row's parent id (`null` at the top), from depth alone.
 * @param {readonly {id: string, depth: number}[]} rows
 * @returns {Map<string, string | null>}
 */
export function parentsFromDepth(rows) {
  const out = new Map();
  const stack = [];
  for (const row of rows) {
    while (stack.length > 0 && stack[stack.length - 1].depth >= row.depth) stack.pop();
    out.set(row.id, stack.length > 0 ? stack[stack.length - 1].id : null);
    stack.push(row);
  }
  return out;
}

/**
 * The ids directly under `parentId` (`null` for the top level), in order.
 * @param {readonly {id: string, depth: number}[]} rows
 * @param {Map<string, string | null>} parents
 * @param {string | null} parentId
 */
export function childrenOf(rows, parents, parentId) {
  return rows.filter((r) => parents.get(r.id) === parentId).map((r) => r.id);
}

/** The index of a row, or -1 — what a scroll-to needs. */
export function rowIndexOf(rows, id) {
  return rows.findIndex((r) => r.id === id);
}

function focusable(r) {
  return r.focusable !== false;
}

/**
 * What a *movement* key means at the cursor, as a description rather than an
 * effect — so the whole keyboard contract is assertable without a DOM.
 *
 * - `ArrowDown` / `ArrowUp`: the next / previous focusable row; from nowhere,
 *   the first / last. Clamped, never wrapped: a wrapping list read aloud
 *   jumps from "last of eleven" to "first" with no boundary spoken.
 * - `Home` / `End`: the first / last.
 * - `PageDown` / `PageUp`: `page` rows on.
 * - `ArrowRight`: opens a closed expandable row; on an open one, steps to its
 *   first child.
 * - `ArrowLeft`: closes an open expandable row; otherwise goes to the parent.
 * - `*`: opens every expandable sibling of the cursor row.
 *
 * Only focusable rows take the cursor: a "listing…" row is a message, and a
 * cursor on it would make a command mean nothing while looking like a
 * selection. `null` means the key is not the tree's — the caller must let it
 * through, or the tree would swallow keys it has no answer for.
 *
 * @param {readonly object[]} rows
 * @param {string | null} cursor the cursor row's id
 * @param {string} key
 * @param {{page?: number}} [opts]
 * @returns {{kind: "cursor", id: string} | {kind: "expand", id: string} | {kind: "collapse", id: string} | {kind: "expandAll", ids: string[]} | null}
 */
export function keyAction(rows, cursor, key, opts = {}) {
  const nav = rows.filter(focusable);
  if (nav.length === 0) return null;
  const i = nav.findIndex((r) => r.id === cursor);
  const at = i === -1 ? null : nav[i];
  const page = Math.max(1, opts.page ?? 10);
  const to = (row) => ({ kind: "cursor", id: row.id });

  switch (key) {
    case "ArrowDown":
      return to(i === -1 ? nav[0] : nav[Math.min(i + 1, nav.length - 1)]);
    case "ArrowUp":
      return to(i === -1 ? nav[nav.length - 1] : nav[Math.max(i - 1, 0)]);
    case "Home":
      return to(nav[0]);
    case "End":
      return to(nav[nav.length - 1]);
    case "PageDown":
      return to(nav[Math.min((i === -1 ? -1 : i) + page, nav.length - 1)]);
    case "PageUp":
      return to(nav[Math.max((i === -1 ? nav.length : i) - page, 0)]);
    case "ArrowRight": {
      if (!at || !at.expandable) return null;
      if (!at.expanded) return { kind: "expand", id: at.id };
      const parents = parentsFromDepth(rows);
      const first = nav[i + 1];
      return first && parents.get(first.id) === at.id ? to(first) : null;
    }
    case "ArrowLeft": {
      if (!at) return null;
      if (at.expandable && at.expanded) return { kind: "collapse", id: at.id };
      const parent = parentsFromDepth(rows).get(at.id);
      const up = parent === null || parent === undefined ? null : nav.find((r) => r.id === parent);
      return up ? to(up) : null;
    }
    case "*": {
      if (!at) return null;
      const parents = parentsFromDepth(rows);
      const parent = parents.get(at.id) ?? null;
      const ids = rows.filter((r) => parents.get(r.id) === parent && r.expandable && !r.expanded).map((r) => r.id);
      return ids.length > 0 ? { kind: "expandAll", ids } : null;
    }
    default:
      return null;
  }
}

/**
 * Type-ahead: the next focusable row after the cursor whose label starts
 * with what was typed, wrapping round; the cursor row itself only when
 * nothing else matches. `null` when no label matches.
 * @param {readonly object[]} rows
 * @param {string | null} cursor
 * @param {string} prefix
 */
export function typeAhead(rows, cursor, prefix) {
  const q = String(prefix ?? "").toLowerCase();
  if (!q) return null;
  const nav = rows.filter(focusable);
  if (nav.length === 0) return null;
  const start = nav.findIndex((r) => r.id === cursor);
  const hit = (r) => String(r.label ?? "").toLowerCase().startsWith(q);
  for (let step = 1; step <= nav.length; step++) {
    const r = nav[(start + step) % nav.length];
    if (hit(r)) return r.id;
  }
  return null;
}

/** Whether `id` is one of `ancestors` or sits anywhere beneath one of them. */
function within(parents, ancestors, id) {
  for (let p = id; p !== null && p !== undefined; p = parents.get(p) ?? null) if (ancestors.has(p)) return true;
  return false;
}

/**
 * Where a drag would land — the sortable-tree projection.
 *
 * `over` is the row under the pointer and `ratio` where the pointer is in it
 * (0 at its top, 1 at its bottom). An expandable row the drag may nest into
 * (`canNest`) takes a drop **inside** across its middle half and a reorder
 * beside it at its edges; any other row splits at the middle. A drop just
 * *below an open row with children* means "first child", the convention every
 * file manager teaches.
 *
 * The plan is `{parent, index, mode, over, depth}`: the parent the dragged
 * rows would join (`null` for the top level), the index among that parent's
 * children with the dragged rows excluded, and `noop` when that is exactly
 * where they already are. `null` when the drop is refused — onto a dragged
 * row itself or into its own subtree, or where `canReorder` says the parent
 * will not take it.
 *
 * `active` is the dragged row's id, or the ids of every row a multi-selection
 * drags together, or `null` for a payload that is not one of these rows, which
 * refuses nothing on its account. One row moving beside its siblings has an
 * exact "already there" (the index); several have only a parent — they are
 * already there when every one of them sits under the target parent.
 *
 * @param {readonly object[]} rows
 * @param {string | readonly string[] | null} active
 * @param {string} over the row under the pointer
 * @param {number} ratio
 * @param {{canNest?: (target: object) => boolean, canReorder?: (parent: object | null) => boolean}} [opts]
 */
export function dropPlan(rows, active, over, ratio, opts = {}) {
  const actives = new Set(active === null ? [] : typeof active === "string" ? [active] : active);
  const target = rows.find((r) => r.id === over);
  if (!target || actives.has(over)) return null;
  const parents = parentsFromDepth(rows);
  if (actives.size > 0 && within(parents, actives, over)) return null;
  const canNest = opts.canNest ?? ((t) => Boolean(t.expandable));
  const canReorder = opts.canReorder ?? (() => true);
  const byId = (id) => (id === null ? null : rows.find((r) => r.id === id) ?? null);
  const r = Math.max(0, Math.min(1, Number(ratio) || 0));
  const nestable = Boolean(target.expandable) && canNest(target);
  const others = (parentId) => childrenOf(rows, parents, parentId).filter((id) => !actives.has(id));

  const finish = (parent, index, mode) => {
    const parentRow = byId(parent);
    if (!canReorder(parentRow)) return null;
    let noop = false;
    if (actives.size === 1) {
      const [one] = actives;
      if ((parents.get(one) ?? null) === parent) {
        const sibs = others(parent);
        const before = childrenOf(rows, parents, parent).indexOf(one);
        noop = mode === "inside" ? false : before === index || (before === sibs.length && index === sibs.length);
      }
    } else if (actives.size > 1) {
      noop = [...actives].every((id) => (parents.get(id) ?? null) === parent);
    }
    return { parent, index, mode, over, depth: parentRow ? parentRow.depth + 1 : 0, noop };
  };

  if (nestable && r >= 0.25 && r <= 0.75) {
    // One row dropped into a folder lands after the folder's own; several
    // land together at its top, where the drop is seen to have taken them.
    return finish(target.id, actives.size > 1 ? 0 : others(target.id).length, "inside");
  }
  const mode = r < 0.5 ? "before" : "after";
  const parent = parents.get(target.id) ?? null;
  if (mode === "after" && nestable && target.expanded && others(target.id).length > 0) {
    return finish(target.id, 0, "after");
  }
  const sibs = others(parent);
  const i = sibs.indexOf(target.id);
  return finish(parent, mode === "before" ? i : i + 1, mode);
}

/**
 * Where the one reorder bar sits for a plan, in the list's own pixels: `x`
 * at the depth the row would take, `y` on the gap — the top of the row the
 * pointer is over for *before*, its bottom for *after*. `null` when there is
 * no gap to mark: a drop inside, one already there, or the root's end.
 * @param {object | null} plan
 * @param {{indent: number, base: number, rowTop: number, rowHeight: number}} geom
 */
export function indicatorStyle(plan, { indent, base, rowTop, rowHeight }) {
  if (!plan || plan.noop || plan.mode === "inside" || plan.over === "") return null;
  return { x: base + plan.depth * indent, y: plan.mode === "before" ? rowTop : rowTop + rowHeight };
}

/**
 * What the drag is being told: nothing while it is elsewhere, `refused` over
 * this tree with no plan, `stay` where it already is, `nest` into a row,
 * `move` into a gap.
 * @param {object | null} plan
 * @param {boolean} overTree the pointer is over this tree at all
 * @returns {"none" | "refused" | "stay" | "nest" | "move"}
 */
export function dragCue(plan, overTree) {
  if (!overTree) return "none";
  if (plan === null) return "refused";
  if (plan.noop) return "stay";
  return plan.mode === "inside" ? "nest" : "move";
}

/**
 * How far a row parts for the gap a plan marks, in pixels: the row above the
 * gap rises, the row below drops, by a hair — the bar is the story, this is
 * the room it takes. `prev` and `next` are the row's neighbours on screen.
 * @param {object | null} plan @param {string} id @param {string | null} prev @param {string | null} next
 */
export function neighbourShift(plan, id, prev, next) {
  if (!plan || plan.noop || plan.mode === "inside" || plan.over === "") return 0;
  if (plan.mode === "before") return id === plan.over ? 2 : next === plan.over ? -2 : 0;
  return id === plan.over ? -2 : prev === plan.over ? 2 : 0;
}

/** Two plans mark the same slot: the same parent and index, both gaps or both inside. */
function sameSlot(a, b) {
  return a.parent === b.parent && a.index === b.index && (a.mode === "inside") === (b.mode === "inside");
}

/**
 * Every slot a keyboard drag can step through, top to bottom: each row's
 * *before*, *inside* and *after* as {@link dropPlan} would answer them, the
 * same slot reached from two rows (after one, before the next) once, and
 * the root's end. The one already-there slot is in the list, so a lift can
 * start from where the row stands.
 * @param {readonly object[]} rows
 * @param {string | readonly string[] | null} active
 * @param {{canNest?: (target: object) => boolean, canReorder?: (parent: object | null) => boolean}} [opts]
 */
export function dropSlots(rows, active, opts = {}) {
  const actives = new Set(active === null ? [] : typeof active === "string" ? [active] : active);
  const out = [];
  const push = (p) => {
    if (p && !out.some((q) => sameSlot(q, p))) out.push(p);
  };
  for (const row of rows) {
    if (actives.has(row.id)) continue;
    for (const ratio of [0.1, 0.5, 0.9]) push(dropPlan(rows, active, row.id, ratio, opts));
  }
  const canReorder = opts.canReorder ?? (() => true);
  if (canReorder(null)) {
    const top = rows.filter((r) => r.depth === 0 && !actives.has(r.id));
    const parents = parentsFromDepth(rows);
    const [one] = actives;
    const already = actives.size === 1 && (parents.get(one) ?? null) === null && rows.filter((r) => r.depth === 0).at(-1)?.id === one;
    push({ parent: null, index: top.length, mode: "inside", over: "", depth: 0, noop: already });
  }
  return out;
}

/**
 * The slot a key moves a keyboard drag to: ↓ and ↑ the next and previous
 * slot, ← and → the same gap one level out or in when there is one (after
 * an open folder is both *its last sibling* and *its first child*); from
 * nowhere, the first. `null` with nothing to step through.
 * @param {readonly object[]} rows @param {readonly object[]} slots @param {object | null} current @param {string} key
 */
export function stepSlot(rows, slots, current, key) {
  if (slots.length === 0) return null;
  const i = current ? slots.findIndex((s) => sameSlot(s, current)) : -1;
  if (i === -1) return slots[0];
  switch (key) {
    case "ArrowDown":
      return slots[Math.min(i + 1, slots.length - 1)];
    case "ArrowUp":
      return slots[Math.max(i - 1, 0)];
    case "ArrowLeft":
    case "ArrowRight": {
      const here = slots[i];
      if (here.mode === "inside") return here;
      // The gap on screen: the line above `over` for *before*, below it for *after*.
      const gapOf = (s) => (s.mode === "inside" ? -1 : rowIndexOf(rows, s.over) + (s.mode === "after" ? 1 : 0));
      const gap = gapOf(here);
      const depth = here.depth + (key === "ArrowRight" ? 1 : -1);
      return slots.find((s) => s.mode !== "inside" && gapOf(s) === gap && s.depth === depth) ?? here;
    }
    default:
      return slots[i];
  }
}
