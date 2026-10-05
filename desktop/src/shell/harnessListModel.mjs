/**
 * When the installed-harness list is read again on its own (`useHarnesses`).
 *
 * The list is read once per window and shared, since a CLI being installed
 * is not something the bus reports. But a read at launch can miss: the node
 * was still coming up, or a cold `--version` outstayed the node's probe and
 * every harness came back "not installed" — and a footer with no installed
 * harness shows nothing, with no refresh to press. So a read that failed, or
 * one that found no harness a person could launch, is asked again on a
 * short, bounded backoff; one that found one is left alone until the bus
 * reconnects or a person presses Refresh. Pure, so `node --test` holds it.
 */

/** The waits before the list is read again, in order; past the last, nothing. */
export const CATALOG_RETRY_MS = Object.freeze([10_000, 30_000, 60_000]);

/**
 * Whether a list is worth reading again: it could not be read (`null`), or
 * no row in it is a harness installed here that a person could launch.
 * @param {readonly {installed: boolean, launch: unknown}[] | null} rows
 */
export function catalogIncomplete(rows) {
  return rows === null || !rows.some((r) => r.installed && r.launch !== null);
}

/**
 * How long until the list is read again — `null` for a list that answered
 * with an installed harness, or once the backoff is spent.
 * @param {readonly {installed: boolean, launch: unknown}[] | null} rows what the last read gave
 * @param {number} attempt how many re-reads were already made after that first read
 * @returns {number | null} milliseconds
 */
export function nextCatalogRead(rows, attempt) {
  if (!catalogIncomplete(rows)) return null;
  return CATALOG_RETRY_MS[attempt] ?? null;
}
