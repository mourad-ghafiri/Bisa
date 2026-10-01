/**
 * What a host's events hold for a person to read (guide/events §Durable,
 * deduplicated, and never in a loop). A payload that came from outside — a
 * public hook's body, an item a connector start's poll listed, a signal an
 * A2A task raised — is read by the content screen before it can begin
 * anything; one the screen would not pass stays **held**, its reason on it,
 * until a person lets it through or leaves it. The host's Inbox row says
 * so; this is where the person acts: the held signals of one host — a
 * library workflow that is On, a goal that listens — oldest first, in
 * words, each with the one verb, *Let it through*
 * (`POST /signals/{id}/release`). Leaving one is doing nothing.
 *
 * From the node's `SignalView` rows alone — never a payload: the node
 * answers none. Pure, so `node --test` reads it; `HeldSignals.tsx` draws it.
 */

import { t } from "../../i18n/l10n.mjs";

/** How many of a host's newest signals are asked for: the node answers 200 at most. */
export const HELD_PAGE = 200;

/**
 * Who listens, as the node names it: `goal:<id>` for a goal — a goal's own
 * design listens through its goal — else `workspace:<id>` for a library
 * workflow; `null` for neither.
 * @param {{workflow?: string | null, goal?: string | null} | null | undefined} of
 */
export function hostOf(of) {
  if (of?.goal) return `goal:${of.goal}`; // for the machine
  if (of?.workflow) return `workspace:${of.workflow}`; // for the machine
  return null;
}

/** What `GET /signals` is asked: one host's, a page of them. @param {string} host */
export function signalsQuery(host) {
  return { host, limit: HELD_PAGE };
}

/** Where a held signal came from, in words — by the kind of event it records. */
const SOURCE_WORDS = Object.freeze({
  hook: () => t("workflow-held-signals-source-hook"),
  connector: () => t("workflow-held-signals-source-connector"),
  signal: (name) => (name ? t("workflow-held-signals-source-signal-named", { name }) : t("workflow-held-signals-source-signal")),
});

function sourceWords(signal) {
  const words = typeof signal.source === "string" && Object.prototype.hasOwnProperty.call(SOURCE_WORDS, signal.source) ? SOURCE_WORDS[signal.source] : null;
  return words ? words(signal.name || null) : t("workflow-held-signals-source-event");
}

/** The step of a listener key, `<host>/<step>`; `null` for a key that is none of this host's. */
function stepOf(listener, host) {
  const prefix = `${host}/`;
  return typeof listener === "string" && listener.startsWith(prefix) ? listener.slice(prefix.length) : null;
}

/**
 * The host's held signals, oldest first — the one that has waited longest
 * leads — each with the start that heard it, where it came from, when, and
 * the node's own reason, whole.
 * @param {readonly object[] | null | undefined} signals `SignalView` rows
 * @param {string | null | undefined} host
 * @returns {{id: string, step: string, source: string, at: number, why: string}[]}
 */
export function heldOf(signals, host) {
  if (!host) return [];
  return (signals ?? [])
    .filter((s) => s?.state === "held" && stepOf(s.listener, host) !== null)
    .map((s) => ({ id: s.id, step: stepOf(s.listener, host), source: sourceWords(s), at: s.at, why: s.note || t("workflow-held-signals-held-for-you") }))
    .sort((a, b) => a.at - b.at || String(a.id).localeCompare(String(b.id)));
}

/** The chip's words: how many wait on the person; `null` when none does — nothing is drawn. @param {number} n */
export function heldWords(n) {
  return n > 0 ? t("workflow-held-signals-held", { n }) : null;
}

/** The facts that move what a host holds: an occurrence written down, one that began its run or was dropped, one refused or held. */
const SIGNAL_FACTS = new Set(["signal_received", "listener_fired", "listener_failed"]);

/**
 * Whether an engine event moves what this host holds — what the list reads
 * again on: a fact about one of the host's listeners.
 * @param {{payload?: {type?: string, listener?: string | null}} | null | undefined} event
 * @param {string | null | undefined} host
 */
export function movesSignalsOf(event, host) {
  const p = event?.payload;
  if (!p || !host || !SIGNAL_FACTS.has(p.type ?? "")) return false;
  return stepOf(p.listener, host) !== null;
}
