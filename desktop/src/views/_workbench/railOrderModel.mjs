/**
 * The person's manual ordering of the project rail, as facts.
 *
 * The order is one machine-scoped `rail.order` setting — a view preference, not
 * a field on the synced project/workstream records — of shape
 * `{ projects: [id…], groups: [name…], workstreams: { [projectId]: [id…] } }`.
 * These helpers read it and produce the next value after a drag; the rail sorts
 * its rows through `orderBy`. Anything not named in the saved order keeps its
 * incoming (name / created-at) order, so a newly created project or workstream
 * is never hidden — it simply lands at the end until it is dragged.
 */

/** The saved project id order, or `[]`. @param {object} order */
export function projectOrder(order) {
  return Array.isArray(order?.projects) ? order.projects : [];
}

/** The saved group-name order, or `[]`. @param {object} order */
export function groupOrder(order) {
  return Array.isArray(order?.groups) ? order.groups : [];
}

/** The saved workstream id order within a project, or `[]`. @param {object} order @param {string} project */
export function workstreamOrder(order, project) {
  const map = order?.workstreams;
  return map && Array.isArray(map[project]) ? map[project] : [];
}

/**
 * Sort `items` so the ids named in `saved` lead, in `saved`'s order, and any
 * item not named keeps its incoming order after them (Array.sort is stable).
 * @param {readonly T[]} items @param {readonly string[]} saved @param {(item: T) => string} idOf
 * @template T
 */
export function orderBy(items, saved, idOf) {
  const rank = new Map((saved ?? []).map((id, i) => [id, i]));
  const at = (item) => (rank.has(idOf(item)) ? rank.get(idOf(item)) : Number.POSITIVE_INFINITY);
  return [...items].sort((a, b) => at(a) - at(b));
}

/**
 * `all` with `id` moved to sit where `index` points among `siblings` — the
 * tree's drop plan turned into a list. `siblings` are the children of the
 * parent the row was dropped into, in their present order; `index` is the
 * position among them with `id` itself excluded (`treeListModel.dropPlan`).
 * `all` is the whole ordered list the setting keeps, which can be longer than
 * one parent's children: the project order is one list across every group,
 * so a project dropped between two of its new group's neighbours takes the
 * slot between those two in the global list. Identity-stable on a no-op.
 * @param {readonly string[]} all
 * @param {readonly string[]} siblings
 * @param {string} id
 * @param {number} index
 */
export function placeAmong(all, siblings, id, index) {
  const rest = all.filter((x) => x !== id);
  const sibs = siblings.filter((x) => x !== id);
  const at = Math.max(0, Math.min(index, sibs.length));
  const following = sibs[at];
  const out = [...rest];
  if (following !== undefined && rest.includes(following)) {
    out.splice(rest.indexOf(following), 0, id);
  } else {
    const preceding = sibs[at - 1];
    if (preceding !== undefined && rest.includes(preceding)) out.splice(rest.indexOf(preceding) + 1, 0, id);
    else out.push(id);
  }
  return out.length === all.length && out.every((x, i) => x === all[i]) ? all : out;
}

/**
 * The next `rail.order` after a drop the tree planned: `id` lands at `index`
 * among `siblings`, in the list `all` of `dimension`. See {@link placeAmong}.
 * @param {object} order @param {{kind:"projects"}|{kind:"groups"}|{kind:"workstreams",project:string}} dimension
 * @param {readonly string[]} all @param {readonly string[]} siblings @param {string} id @param {number} index
 */
export function reorderAmong(order, dimension, all, siblings, id, index) {
  const next = placeAmong(all, siblings, id, index);
  if (next === all) return order;
  return withList(order, dimension, next);
}

/** `order` with the list of `dimension` replaced. */
function withList(order, dimension, next) {
  const base = order && typeof order === "object" ? order : {};
  switch (dimension.kind) {
    case "projects":
      return { ...base, projects: next };
    case "groups":
      return { ...base, groups: next };
    case "workstreams":
      return { ...base, workstreams: { ...(base.workstreams ?? {}), [dimension.project]: next } };
    default:
      return order;
  }
}

/**
 * The order after a group is renamed: the name is replaced in place
 * in the `groups` list so the group keeps its manual position. A no-op when the
 * group is not manually ordered or the names match.
 * @param {object} order @param {string} old @param {string} next
 */
export function renameGroup(order, old, next) {
  const groups = groupOrder(order);
  if (old === next || !groups.includes(old)) return order;
  const base = order && typeof order === "object" ? order : {};
  return { ...base, groups: groups.map((g) => (g === old ? next : g)) };
}
