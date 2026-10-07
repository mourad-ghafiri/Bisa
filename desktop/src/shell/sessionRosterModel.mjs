/**
 * The session roster's arithmetic, with no store and no bus in it: how a
 * frame moves a row, how a read of the whole roster lands beside the frames
 * that arrived while it was out, and what a *Stop* the node answered means.
 * `sessionsStore.ts` holds the roster and the wires; every decision is here.
 *
 * Plain `.mjs` with a `.d.mts` beside it, so `node --test` reads it.
 */

import { saidAfter } from "./stopOutcomeModel.mjs";
import { t } from "../i18n/l10n.mjs";

/**
 * The newer of two words about one row, by the roster's `revision` — the
 * number the node bumps on every change it says, so two frames of one row
 * are ordered however they reached the desktop. A row without one (an older
 * node's) is never older; equal, or unknown, the later word stands.
 * @template {{revision?: number}} R
 * @param {R} held what the desktop holds @param {R} said what arrived
 * @returns {R}
 */
export function latest(held, said) {
  const older = typeof said.revision === "number" && typeof held.revision === "number" && said.revision < held.revision;
  return older ? held : said;
}

/**
 * The roster with one row as a `session_state` frame says it: replaced where
 * it stood, else first — the newest session leads until the order is drawn.
 * A frame older than the row held (`latest`) moves nothing: the same array
 * comes back, so the store neither commits nor announces a transition to a
 * word the row already left.
 * @template {{id: string, revision?: number}} R
 * @param {readonly R[]} sessions @param {R} row
 * @returns {readonly R[]}
 */
export function upserted(sessions, row) {
  const held = sessions.find((s) => s.id === row.id);
  if (!held) return [row, ...sessions];
  if (latest(held, row) === held) return sessions;
  return sessions.map((s) => (s.id === row.id ? row : s));
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
 * A read of the whole roster, as it lands, beside every frame that arrived
 * while the read was out. For a row both name, the newer word by `revision`
 * stands (`latest`): a read asked for before a frame and answered after it
 * carries the later number and wins; one answered from before the frame does
 * not, so a session a frame said was *aborted* never reads *running* again
 * because a snapshot asked for a moment earlier answered a moment later. One
 * a frame said was gone does not come back, whatever the read says.
 * @template {{id: string, revision?: number}} R
 * @param {readonly R[]} snapshot what `GET /sessions` answered
 * @param {ReadonlyMap<string, R | null>} since the rows frames moved since the read was asked for — the row, or `null` for one that went
 * @returns {R[]}
 */
export function landedRead(snapshot, since) {
  if (since.size === 0) return [...snapshot];
  const kept = snapshot.filter((row) => !since.has(row.id));
  const read = new Map(snapshot.map((row) => [row.id, row]));
  const fresh = [...since.entries()]
    .map(([id, heard]) => {
      if (heard === null) return null;
      // The frame is the later word unless both carry a revision and the
      // read's is higher — a read that answered after the frame.
      const row = read.get(id);
      return row ? latest(row, heard) : heard;
    })
    .filter((row) => row !== null);
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
export function stopWords(outcome, stopped, ended = null) {
  if (outcome === "gone") return t("shell-sessions-already-ended");
  // The node's word on what the stop ended rides after the surface's: a
  // harness that had to be terminated, a session that could not be ended.
  return saidAfter(stopped, ended, { sessionsSaid: true });
}

/**
 * The rows a read of the whole roster moved, beside the roster held before
 * it — each as a transition a frame would have announced: `[prev, row]`
 * for a row whose state changed, `[null, row]` for one that appeared.
 * @template {{id: string, state: {state: string}}} R
 * @param {readonly R[]} before @param {readonly R[]} after
 * @returns {[R["state"] | null, R][]}
 */
export function transitionsBetween(before, after) {
  const held = new Map(before.map((r) => [r.id, r]));
  const moved = [];
  for (const row of after) {
    const prev = held.get(row.id);
    if (!prev) moved.push([null, row]);
    else if (prev.state.state !== row.state.state) moved.push([prev.state, row]);
  }
  return moved;
}
