/**
 * The arithmetic of a sortable list (ide/03), with no DOM in it: where a
 * dragged item lands, how a list moves one element, and which item follows
 * another. The strips and the rail ask; this answers.
 */

/**
 * The index an item takes when dropped over another item of the same list —
 * the sortable convention: the dragged item takes the place of the one it is
 * dropped on, and everything between shifts by one. `null` when either id is
 * not in the list or nothing would move.
 * @param {readonly string[]} ids the list in its present order
 * @param {string} activeId the dragged item
 * @param {string} overId the item under the pointer
 * @returns {{from: number, to: number} | null}
 */
export function sortableDrop(ids, activeId, overId) {
  const from = ids.indexOf(activeId);
  const to = ids.indexOf(overId);
  if (from === -1 || to === -1 || from === to) return null;
  return { from, to };
}

/**
 * Move one element of a list to `index`. Identity-stable: the same array when
 * nothing moves, so a store can skip the update.
 * @template T
 * @param {readonly T[]} list
 * @param {number} from
 * @param {number} to
 */
export function moveIndex(list, from, to) {
  if (from < 0 || from >= list.length) return list;
  const target = Math.max(0, Math.min(to, list.length - 1));
  if (from === target) return list;
  const out = [...list];
  const [item] = out.splice(from, 1);
  out.splice(target, 0, item);
  return out;
}

/**
 * The id that follows (`1`) or precedes (`-1`) `active` in a strip, wrapping
 * at the ends — `next_tab` and `prev_tab`. Null for an empty strip; the first
 * id when nothing is active.
 * @param {readonly string[]} ids
 * @param {string | null} active
 * @param {1 | -1} dir
 */
export function cycle(ids, active, dir) {
  if (ids.length === 0) return null;
  const at = active === null ? -1 : ids.indexOf(active);
  if (at === -1) return ids[0];
  return ids[(at + dir + ids.length) % ids.length];
}

/**
 * The one spelling of a sortable item's id. A list's own items are
 * `<list>/<id>`; the items of a **family** — lists a card may move between
 * while it is dragged, a Kanban's columns — are `<family>/<id>`, the same
 * whichever list holds the item, so the drag keeps tracking it as it moves.
 * @param {string | null | undefined} family
 * @param {string} listId
 * @param {string} id
 */
export function sortableId(family, listId, id) {
  return `${family || listId}/${id}`;
}

/**
 * The slot a foreign item hovering a list would take: the slot of the item
 * it is over, the end for the list's well or an item the list does not
 * know, and — should the item it is over be itself — where it already is.
 * @param {readonly string[]} ids the list in its present order
 * @param {string | null | undefined} overId the item under the pointer, or null for the well
 * @param {string} activeId the hovering item
 */
export function hoverIndex(ids, overId, activeId) {
  if (!overId) return ids.length;
  const at = ids.indexOf(overId);
  if (at === -1) return ids.length;
  // Over itself — the list already holds it — is where it already is.
  void activeId;
  return at;
}
