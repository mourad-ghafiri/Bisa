/**
 * A pane tree: a binary tree whose leaves hold tab sets (ide/06).
 *
 * Built for the terminal panel and shaped so the document area can adopt it
 * later — the spec asks for one model, tested once. Plain JavaScript, no
 * React: which tab sits in which pane is the state that decides which PTY is
 * visible where, and a tab that silently changed pane is a shell somebody
 * cannot find.
 *
 * Two rules the whole file keeps:
 *
 * 1. **Every tab appears in exactly one leaf.** `normalize` enforces it and
 *    every mutation calls it, so a caller cannot produce a tree with a tab in
 *    two panes or in none.
 * 2. **Closing a pane never drops a tab.** The pane's tabs move to the nearest
 *    surviving leaf. A dropped tab would be an unmounted terminal, and an
 *    unmounted terminal is a terminated shell.
 *
 * Ids (`p<n>`, `s<n>`) come from a counter the caller owns and are never
 * reused; nothing parses them, so the delimiter rule of ide/06 §Session ids
 * is met by having no components at all.
 */

/** @typedef {{kind: "leaf", id: string, tabs: string[], active: string | null}} Leaf */
/** @typedef {{kind: "split", id: string, dir: "row" | "col", ratio: number, a: Node, b: Node}} Split */
/** @typedef {Leaf | Split} Node */

const MIN_RATIO = 0.15;
const MAX_RATIO = 0.85;

/** @param {string} id @param {string[]} [tabs] @param {string | null} [active] @returns {Leaf} */
export function singleLeaf(id, tabs = [], active = null) {
  return { kind: "leaf", id, tabs: [...tabs], active: active ?? tabs[0] ?? null };
}

/** Every leaf, left-to-right / top-to-bottom. @param {Node} tree @returns {Leaf[]} */
export function leaves(tree) {
  if (!tree) return [];
  if (tree.kind === "leaf") return [tree];
  return [...leaves(tree.a), ...leaves(tree.b)];
}

/** @param {Node} tree @param {string} leafId @returns {Leaf | null} */
export function findLeaf(tree, leafId) {
  return leaves(tree).find((l) => l.id === leafId) ?? null;
}

/** The leaf holding a tab, or null. @param {Node} tree @param {string} tabKey @returns {Leaf | null} */
export function leafOfTab(tree, tabKey) {
  return leaves(tree).find((l) => l.tabs.includes(tabKey)) ?? null;
}


/**
 * Rebuild the tree with one node replaced. Returns the same object when the
 * replacement is identical, so identity keeps meaning "changed".
 * @param {Node} tree @param {string} id @param {(node: Node) => Node} f
 */
function mapNode(tree, id, f) {
  if (tree.id === id) return f(tree);
  if (tree.kind === "leaf") return tree;
  const a = mapNode(tree.a, id, f);
  const b = mapNode(tree.b, id, f);
  if (a === tree.a && b === tree.b) return tree;
  return { ...tree, a, b };
}

/**
 * Enforce the two rules: no duplicate tabs (the first occurrence wins), no
 * empty leaf while another leaf exists, every leaf's `active` a member of its
 * tabs. Idempotent; returns the same object when nothing needed fixing.
 * @param {Node} tree @returns {Node}
 */
export function normalize(tree) {
  const seen = new Set();
  let changed = false;
  /** @param {Node} n @returns {Node} */
  const dedupe = (n) => {
    if (n.kind === "leaf") {
      const tabs = n.tabs.filter((t) => {
        if (seen.has(t)) return false;
        seen.add(t);
        return true;
      });
      const active = n.active !== null && tabs.includes(n.active) ? n.active : (tabs[tabs.length - 1] ?? null);
      if (tabs.length === n.tabs.length && active === n.active) return n;
      changed = true;
      return { ...n, tabs, active };
    }
    const a = dedupe(n.a);
    const b = dedupe(n.b);
    return a === n.a && b === n.b ? n : { ...n, a, b };
  };
  let out = dedupe(tree);
  /** @param {Node} n @returns {Node} */
  const collapse = (n) => {
    if (n.kind === "leaf") return n;
    const a = collapse(n.a);
    const b = collapse(n.b);
    if (a.kind === "leaf" && a.tabs.length === 0) {
      changed = true;
      return b;
    }
    if (b.kind === "leaf" && b.tabs.length === 0) {
      changed = true;
      return a;
    }
    return a === n.a && b === n.b ? n : { ...n, a, b };
  };
  out = collapse(out);
  return changed ? out : tree;
}

/**
 * Put a tab in a leaf (removing it from any other), optionally making it the
 * leaf's active tab. An unknown leaf is a no-op.
 * @param {Node} tree @param {string} leafId @param {string} tabKey @param {boolean} [activate]
 */
export function addTab(tree, leafId, tabKey, activate = true) {
  if (!findLeaf(tree, leafId)) return tree;
  const from = leafOfTab(tree, tabKey);
  let out = tree;
  if (from && from.id !== leafId) out = removeTab(out, tabKey, false);
  else if (from && from.id === leafId) {
    return activate && from.active !== tabKey ? setActiveTab(tree, leafId, tabKey) : tree;
  }
  out = mapNode(out, leafId, (n) =>
    n.kind === "leaf" ? { ...n, tabs: [...n.tabs, tabKey], active: activate ? tabKey : (n.active ?? tabKey) } : n,
  );
  return normalize(out);
}

/**
 * Take a tab out. The leaf's active tab moves to the right neighbour, then the
 * left. With `collapse` (the default) an emptied leaf disappears when another
 * leaf exists.
 * @param {Node} tree @param {string} tabKey @param {boolean} [collapse]
 */
export function removeTab(tree, tabKey, collapse = true) {
  const leaf = leafOfTab(tree, tabKey);
  if (!leaf) return tree;
  const at = leaf.tabs.indexOf(tabKey);
  const tabs = leaf.tabs.filter((t) => t !== tabKey);
  const active = leaf.active === tabKey ? (tabs[at] ?? tabs[at - 1] ?? null) : leaf.active;
  const out = mapNode(tree, leaf.id, (n) => (n.kind === "leaf" ? { ...n, tabs, active } : n));
  return collapse ? normalize(out) : out;
}

/**
 * Put a tab at `index` within its own leaf — a drag along the strip. The
 * tab keeps its pane and its active bit; an index past the end is the end.
 * Identity-stable when nothing moves.
 * @param {Node} tree @param {string} tabKey @param {number} index
 */
export function moveWithin(tree, tabKey, index) {
  const leaf = leafOfTab(tree, tabKey);
  if (!leaf) return tree;
  const from = leaf.tabs.indexOf(tabKey);
  const to = Math.max(0, Math.min(index, leaf.tabs.length - 1));
  if (from === to) return tree;
  const tabs = [...leaf.tabs];
  tabs.splice(from, 1);
  tabs.splice(to, 0, tabKey);
  return mapNode(tree, leaf.id, (n) => (n.kind === "leaf" ? { ...n, tabs } : n));
}

/** @param {Node} tree @param {string} leafId @param {string} tabKey */
export function setActiveTab(tree, leafId, tabKey) {
  const leaf = findLeaf(tree, leafId);
  if (!leaf || !leaf.tabs.includes(tabKey) || leaf.active === tabKey) return tree;
  return mapNode(tree, leafId, (n) => (n.kind === "leaf" ? { ...n, active: tabKey } : n));
}

/**
 * Split a leaf. The leaf keeps its tabs on the first side; the new leaf on the
 * second side starts empty (the caller puts a tab in it before the next
 * normalize) or, with `moveActive`, takes the leaf's active tab.
 * @param {Node} tree @param {string} leafId @param {"row" | "col"} dir @param {string} splitId @param {string} newLeafId @param {boolean} [moveActive]
 */
export function splitLeaf(tree, leafId, dir, splitId, newLeafId, moveActive = false) {
  const leaf = findLeaf(tree, leafId);
  if (!leaf) return tree;
  if (moveActive && (leaf.active === null || leaf.tabs.length < 2)) return tree;
  const moved = moveActive ? leaf.active : null;
  const rest = moved ? leaf.tabs.filter((t) => t !== moved) : leaf.tabs;
  const at = moved ? leaf.tabs.indexOf(moved) : -1;
  const a = moved ? { ...leaf, tabs: rest, active: rest[at] ?? rest[at - 1] ?? null } : leaf;
  const b = singleLeaf(newLeafId, moved ? [moved] : []);
  return mapNode(tree, leafId, () => ({ kind: "split", id: splitId, dir, ratio: 0.5, a, b }));
}

/**
 * Close a pane: its tabs move to the nearest surviving leaf (the sibling's
 * first leaf) and the split collapses. The only leaf cannot be closed.
 * @param {Node} tree @param {string} leafId
 */
export function closeLeaf(tree, leafId) {
  const leaf = findLeaf(tree, leafId);
  if (!leaf || tree.kind === "leaf") return tree;
  /** @param {Node} n @returns {Node} */
  const go = (n) => {
    if (n.kind === "leaf") return n;
    if (n.a.id === leafId || n.b.id === leafId) {
      const gone = n.a.id === leafId ? n.a : n.b;
      const keep = n.a.id === leafId ? n.b : n.a;
      const into = leaves(keep)[0];
      const moving = /** @type {Leaf} */ (gone).tabs;
      if (moving.length === 0) return keep;
      return mapNode(keep, into.id, (l) =>
        l.kind === "leaf" ? { ...l, tabs: [...l.tabs, ...moving], active: l.active ?? moving[0] } : l,
      );
    }
    const a = go(n.a);
    const b = go(n.b);
    return a === n.a && b === n.b ? n : { ...n, a, b };
  };
  return normalize(go(tree));
}

/** @param {Node} tree @param {string} splitId @param {number} ratio */
export function setRatio(tree, splitId, ratio) {
  const r = Math.min(MAX_RATIO, Math.max(MIN_RATIO, ratio));
  return mapNode(tree, splitId, (n) => (n.kind === "split" && n.ratio !== r ? { ...n, ratio: r } : n));
}

/**
 * Where each leaf sits, as fractions of the panel: `{leafId, x, y, w, h}`.
 * Pure arithmetic over the ratios, so the renderer never has to measure
 * before it can place a terminal — and every terminal can live in one flat
 * layer, positioned over its pane, which is what lets a tab move between
 * panes without remounting (and ending) its shell.
 * @param {Node} tree @returns {{leafId: string, x: number, y: number, w: number, h: number}[]}
 */
export function rects(tree) {
  /** @type {{leafId: string, x: number, y: number, w: number, h: number}[]} */
  const out = [];
  const walk = (n, x, y, w, h) => {
    if (n.kind === "leaf") {
      out.push({ leafId: n.id, x, y, w, h });
      return;
    }
    if (n.dir === "row") {
      walk(n.a, x, y, w * n.ratio, h);
      walk(n.b, x + w * n.ratio, y, w * (1 - n.ratio), h);
    } else {
      walk(n.a, x, y, w, h * n.ratio);
      walk(n.b, x, y + h * n.ratio, w, h * (1 - n.ratio));
    }
  };
  if (tree) walk(tree, 0, 0, 1, 1);
  return out;
}

/**
 * Where the draggable dividers are: for a row split a vertical line at the
 * ratio, for a col split a horizontal one, each with the rect of the split it
 * belongs to so a drag can turn pixels back into a ratio.
 * @param {Node} tree
 * @returns {{splitId: string, dir: "row" | "col", x: number, y: number, w: number, h: number, at: number}[]}
 */
export function dividers(tree) {
  const out = [];
  const walk = (n, x, y, w, h) => {
    if (n.kind === "leaf") return;
    out.push({ splitId: n.id, dir: n.dir, x, y, w, h, at: n.dir === "row" ? x + w * n.ratio : y + h * n.ratio });
    if (n.dir === "row") {
      walk(n.a, x, y, w * n.ratio, h);
      walk(n.b, x + w * n.ratio, y, w * (1 - n.ratio), h);
    } else {
      walk(n.a, x, y, w, h * n.ratio);
      walk(n.b, x, y + h * n.ratio, w, h * (1 - n.ratio));
    }
  };
  if (tree) walk(tree, 0, 0, 1, 1);
  return out;
}

/**
 * The leaf next to `leafId` in a direction — the one whose rect is adjacent
 * on that side with the most overlap — or null at the edge.
 * @param {Node} tree @param {string} leafId @param {"left" | "right" | "up" | "down"} direction
 */
export function neighbor(tree, leafId, direction) {
  const all = rects(tree);
  const me = all.find((r) => r.leafId === leafId);
  if (!me) return null;
  const eps = 1e-6;
  const overlap = (a0, a1, b0, b1) => Math.max(0, Math.min(a1, b1) - Math.max(a0, b0));
  let best = null;
  let bestOverlap = 0;
  for (const r of all) {
    if (r.leafId === leafId) continue;
    let touching = false;
    let o = 0;
    if (direction === "right" && Math.abs(r.x - (me.x + me.w)) < eps) {
      touching = true;
      o = overlap(me.y, me.y + me.h, r.y, r.y + r.h);
    } else if (direction === "left" && Math.abs(r.x + r.w - me.x) < eps) {
      touching = true;
      o = overlap(me.y, me.y + me.h, r.y, r.y + r.h);
    } else if (direction === "down" && Math.abs(r.y - (me.y + me.h)) < eps) {
      touching = true;
      o = overlap(me.x, me.x + me.w, r.x, r.x + r.w);
    } else if (direction === "up" && Math.abs(r.y + r.h - me.y) < eps) {
      touching = true;
      o = overlap(me.x, me.x + me.w, r.x, r.x + r.w);
    }
    if (touching && o > bestOverlap) {
      bestOverlap = o;
      best = r.leafId;
    }
  }
  return best;
}

/**
 * Read a tree back from storage, keeping only tabs in `validTabs` and
 * dropping anything malformed. `null` when nothing usable is there.
 * @param {unknown} json @param {Iterable<string>} validTabs @returns {Node | null}
 */
export function parseTree(json, validTabs) {
  const valid = new Set(validTabs);
  /** @param {any} n @returns {Node | null} */
  const read = (n) => {
    if (!n || typeof n !== "object" || typeof n.id !== "string") return null;
    if (n.kind === "leaf") {
      const tabs = Array.isArray(n.tabs) ? n.tabs.filter((t) => typeof t === "string" && valid.has(t)) : [];
      return singleLeaf(n.id, tabs, typeof n.active === "string" && tabs.includes(n.active) ? n.active : null);
    }
    if (n.kind === "split" && (n.dir === "row" || n.dir === "col")) {
      const a = read(n.a);
      const b = read(n.b);
      if (!a || !b) return a ?? b;
      const ratio = typeof n.ratio === "number" && Number.isFinite(n.ratio) ? n.ratio : 0.5;
      return { kind: "split", id: n.id, dir: n.dir, ratio: Math.min(MAX_RATIO, Math.max(MIN_RATIO, ratio)), a, b };
    }
    return null;
  };
  const tree = read(json);
  return tree ? normalize(tree) : null;
}
