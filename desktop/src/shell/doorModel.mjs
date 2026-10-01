/**
 * A door is a request from anywhere in the app to a dialog one screen
 * hosts — *New goal* to the Goals screen, *New workstream* to the
 * workbench. `shortcuts.fire` navigates to the hosting screen and
 * dispatches; but a screen is a lazy chunk, and one macrotask does not
 * cover a dynamic import, so a request fired before its listener mounted
 * used to vanish with no error and no dialog. This rule keeps one pending
 * request per door until a listener registers and takes it, and drops
 * one nobody took within `DOOR_TTL_MS` — a door pressed and forgotten must
 * not open a dialog a minute later on a screen someone opened for another
 * reason.
 */

/** How long a request waits for its listener. */
export const DOOR_TTL_MS = 10_000;

/** @typedef {{ pending: Record<string, { detail: unknown, at: number }> }} Doors */

/** @returns {Doors} */
export function emptyDoors() {
  return { pending: {} };
}

/**
 * Keep a request for a door nobody listens to yet; a newer request for
 * the same door replaces the older.
 *
 * @param {Doors} doors
 * @param {string} event
 * @param {unknown} detail
 * @param {number} now unix milliseconds
 * @returns {Doors}
 */
export function pendDoor(doors, event, detail, now) {
  return { pending: { ...doors.pending, [event]: { detail, at: now } } };
}

/**
 * A listener mounted: hand it the request waiting for its door, if one is
 * fresh. A stale one is dropped either way.
 *
 * @param {Doors} doors
 * @param {string} event
 * @param {number} now unix milliseconds
 * @returns {{ doors: Doors, request: { detail: unknown } | null }}
 */
export function takeDoor(doors, event, now) {
  const waiting = doors.pending[event];
  if (!waiting) return { doors, request: null };
  const { [event]: _taken, ...rest } = doors.pending;
  const fresh = now - waiting.at <= DOOR_TTL_MS;
  return { doors: { pending: rest }, request: fresh ? { detail: waiting.detail } : null };
}
