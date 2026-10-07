/**
 * What a stop, a restart, a close, a retirement or an abort ended, in one
 * sentence — the node's `ended` block (`sessions, terminated, still_live,
 * children`) worded once for every toast and confirm on the desktop: the
 * sessions stopped; the harnesses that ignored it and were terminated at the
 * deadline; the sessions that could not be ended; the spawned goals stopped
 * or closed with the thing. A node that answers no block (an older one)
 * says nothing more than the lead.
 *
 * Plain `.mjs` with a `.d.mts` beside it, so `node --test` reads it.
 */

import { t as tr } from "../i18n/l10n.mjs";

/** The block of a verb that ended nothing. */
export const NOTHING_ENDED = Object.freeze({ sessions: 0, terminated: 0, still_live: 0, children: Object.freeze([]) });

/**
 * The block a verb's answer carries — on the answer itself, or under its
 * `retired` — read tolerantly; `null` for an older node that says none.
 * @param {unknown} answer
 * @returns {{sessions: number, terminated: number, still_live: number, children: string[]} | null}
 */
export function endedOf(answer) {
  const a = /** @type {{ended?: unknown, retired?: {ended?: unknown}} | null} */ (answer);
  const raw = a?.ended ?? a?.retired?.ended ?? null;
  if (!raw || typeof raw !== "object") return null;
  const e = /** @type {Record<string, unknown>} */ (raw);
  const n = (v) => (typeof v === "number" && Number.isFinite(v) ? v : 0);
  return { sessions: n(e.sessions), terminated: n(e.terminated), still_live: n(e.still_live), children: Array.isArray(e.children) ? e.children.map(String) : [] };
}

/**
 * The parts of the sentence, in the order they are said; empty when the
 * block ended nothing worth a word. `sessionsSaid` drops the first part for
 * a surface whose lead already said the session stopped; `closed` says the
 * children were closed rather than stopped.
 * @param {{sessions: number, terminated: number, still_live: number, children: readonly string[]} | null | undefined} ended
 * @param {{closed?: boolean, sessionsSaid?: boolean}} [opts]
 * @returns {string[]}
 */
export function endedParts(ended, { closed = false, sessionsSaid = false } = {}) {
  if (!ended) return [];
  const parts = [];
  if (!sessionsSaid && ended.sessions > 0) parts.push(tr("shell-stop-outcome-sessions-stopped", { n: ended.sessions }));
  if (ended.terminated > 0) parts.push(tr("shell-stop-outcome-terminated", { n: ended.terminated }));
  if (ended.still_live > 0) parts.push(tr("shell-stop-outcome-still-live", { n: ended.still_live }));
  const children = ended.children.length;
  if (children > 0 && closed) parts.push(tr("shell-stop-outcome-children-closed", { n: children }));
  if (children > 0 && !closed) parts.push(tr("shell-stop-outcome-children-stopped", { n: children }));
  return parts;
}

/** The parts as one clause — *a, b and c* — or `null` when there are none. */
export function endedWords(ended, opts) {
  const parts = endedParts(ended, opts);
  if (parts.length === 0) return null;
  return parts.reduce((said, part, i) => (i === 0 ? part : i === parts.length - 1 ? tr("shell-stop-outcome-and", { a: said, b: part }) : tr("shell-stop-outcome-comma", { a: said, b: part })), "");
}

/**
 * The toast: the surface's lead, and after it what was ended — or the lead
 * alone when nothing was, or the node said nothing.
 * @param {string} lead @param {Parameters<typeof endedParts>[0]} ended @param {Parameters<typeof endedParts>[1]} [opts]
 */
export function saidAfter(lead, ended, opts) {
  const words = endedWords(ended, opts);
  if (!words) return lead;
  return lead ? tr("shell-stop-outcome-said", { lead, ended: words }) : words;
}

/** The toast's tone: a note when something could not be ended, a success otherwise. */
export function stopTone(ended) {
  return ended && ended.still_live > 0 ? "info" : "ok";
}

/**
 * What a confirm promises before the verb: the sessions working on the
 * thing end; the goals it spawned are stopped — or closed — with it. `null`
 * when it names neither.
 * @param {{sessions?: number, children?: number, closing?: boolean}} [facts]
 */
export function willStopWords({ sessions = 0, children = 0, closing = false } = {}) {
  const parts = [];
  if (sessions > 0) parts.push(tr("shell-stop-outcome-will-stop-sessions", { n: sessions }));
  if (children > 0 && closing) parts.push(tr("shell-stop-outcome-will-stop-children-closed", { n: children }));
  if (children > 0 && !closing) parts.push(tr("shell-stop-outcome-will-stop-children-stopped", { n: children }));
  return parts.length > 0 ? parts.join(" ") : null;
}
