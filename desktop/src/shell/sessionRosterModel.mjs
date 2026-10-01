/**
 * The session roster's arithmetic, with no store and no bus in it: how a
 * frame moves a row, how a read of the whole roster lands beside the frames
 * that arrived while it was out, and what a *Stop* the node answered means.
 * `sessionsStore.ts` holds the roster and the wires; every decision is here.
 *
 * Plain `.mjs` with a `.d.mts` beside it, so `node --test` reads it.
 */

import { t } from "../i18n/l10n.mjs";

/**
 * The roster with one row as a `session_state` frame says it: replaced where
 * it stood, else first — the newest session leads until the order is drawn.
 * @template {{id: string}} R
 * @param {readonly R[]} sessions @param {R} row
 * @returns {R[]}
 */
export function upserted(sessions, row) {
  return sessions.some((s) => s.id === row.id) ? sessions.map((s) => (s.id === row.id ? row : s)) : [row, ...sessions];
}

/**
 * The roster without a row — a `session_gone` frame, or a *Stop* the node
 * answered *no such session*; the same array when it holds none.
 * @template {{id: string}} R
 * @param {readonly R[]} sessions @param {string} id
 * @returns {readonly R[]}
 */
export function dropped(sessions, id) {
  return sessions.some((s) => s.id === id) ? sessions.filter((s) => s.id !== id) : sessions;
}

/**
 * A read of the whole roster, as it lands: the node's rows — **under** every
 * frame that arrived while the read was out. The read was taken before those
 * frames were said, so for the rows they name it is the older word: a session
 * a frame said was *aborted* must not read *running* again because a
 * snapshot asked for a moment earlier answered a moment later, and one a
 * frame said was gone must not come back.
 * @template {{id: string}} R
 * @param {readonly R[]} snapshot what `GET /sessions` answered
 * @param {ReadonlyMap<string, R | null>} since the rows frames moved since the read was asked for — the row, or `null` for one that went
 * @returns {R[]}
 */
export function landedRead(snapshot, since) {
  if (since.size === 0) return [...snapshot];
  const kept = snapshot.filter((row) => !since.has(row.id));
  const fresh = [...since.values()].filter((row) => row !== null);
  return [...fresh.reverse(), ...kept];
}

/**
 * What a *Stop* that threw means: the node holds no such session (its 404 —
 * the row outlived the session, a `session_gone` the stream lost), so the
 * row is dropped and nothing failed; anything else is a failure, said.
 * @param {unknown} error what `POST /sessions/{id}/abort` threw
 * @returns {boolean}
 */
export function stoppedAlready(error) {
  return !!error && typeof error === "object" && /** @type {{status?: unknown}} */ (error).status === 404;
}

/**
 * The toast after a *Stop*: the caller's own sentence when the node stopped
 * it, and the plain fact when there was nothing left to stop — never an
 * error for a session that had already ended.
 * @param {"stopped" | "gone"} outcome @param {string} stopped the surface's sentence for a session it stopped
 */
export function stopWords(outcome, stopped) {
  return outcome === "gone" ? t("shell-sessions-already-ended") : stopped;
}
