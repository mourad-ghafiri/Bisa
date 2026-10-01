/**
 * Where the next, older page of a timeline continues from (13 —
 * Conversations; `PageBefore` in `bisa-store`). Messages are stamped in
 * whole seconds, and two posted within one share theirs, so a page cut by
 * the second alone drops the rest of that second.
 *
 * - The node takes the oldest row's id beside its moment and cuts exactly
 *   there: nothing is lost, nothing is read twice.
 * - A hosted channel is read over the guest wire, which knows the second
 *   alone: the cursor asks for one second *later* than the oldest row, and
 *   the caller's id-merge discards the overlap. Lossless too, at the cost of
 *   re-reading up to a second's worth.
 */

/**
 * @param {{id: string, created_at: number}} oldest the oldest row shown
 * @param {boolean} hosted whether the scope is read over the guest wire
 * @returns {{at: number, id?: string}}
 */
export function olderThan(oldest, hosted) {
  return hosted ? { at: oldest.created_at + 1 } : { at: oldest.created_at, id: oldest.id };
}
