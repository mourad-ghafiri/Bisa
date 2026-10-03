/**
 * Edits typed into a Settings form and not yet saved — the one fact the
 * screen needs to decide whether leaving a panel loses work.
 *
 * Most of Settings writes as you go (a switch, a registry row on blur), so
 * there is nothing to lose. A few forms are explicit-save — *Who commits*,
 * your identity, a security rule list — and a panel switch unmounts them:
 * their draft goes with the panel. Each such form says whether it holds
 * unsaved edits under its own id; the rail asks before it moves while any
 * does, and says nothing otherwise.
 */

/**
 * The set of forms holding unsaved edits, with `id` marked as `dirty` or
 * clean. A new set when it changed, the same one when it did not, so a
 * store can skip telling anyone.
 *
 * @param {ReadonlySet<string>} forms
 * @param {string} id
 * @param {boolean} dirty
 */
export function markForm(forms, id, dirty) {
  if (forms.has(id) === dirty) return forms;
  const next = new Set(forms);
  if (dirty) next.add(id);
  else next.delete(id);
  return next;
}

/**
 * Whether going from panel `from` to panel `to` asks first: only when it
 * moves, and only while a form holds unsaved edits.
 *
 * @param {ReadonlySet<string>} forms
 * @param {string} from
 * @param {string} to
 */
export function leaveAsks(forms, from, to) {
  return from !== to && forms.size > 0;
}
