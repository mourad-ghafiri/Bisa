/**
 * The bus's facts (`bus.ts` holds the `EventSource`): which frames a
 * subscriber hears, what a frame must look like to be one, what the
 * connection's word is after each thing that happens to the stream, what a
 * stream's end earns in the log, how long to wait before connecting again,
 * and when a frame that could not be read is worth a line in the log. Plain
 * `.mjs`, so `node --test` reads it.
 */

/** The first wait before a reconnect; each failure doubles it. */
export const BASE_RETRY_MS = 500;

/** The longest wait between two connection attempts — a node that is down does not spin. */
export const MAX_RETRY_MS = 15_000;

/** How far the wait is spread around its step — two windows dropped by one restart do not knock together. */
const RETRY_JITTER = 0.25;

/** One line per window for frames that could not be read, never one per frame. */
export const MALFORMED_WARN_WINDOW_MS = 60_000;

/** The connection's word before anything was tried: nothing is known to be away. */
export const FIRST_CONN = "connecting";

/**
 * The connection's word after something happened to the stream.
 *
 * - `opened` is `open`.
 * - `ended` is `closed`. An `EventSource` says `error` for every way a stream
 *   ends — the node was asked to stop and ended it, the connection broke, an
 *   attempt found nobody — and each is **the node being away**, never an
 *   error to show: the bus closes, waits (`retryDelay`) and asks again until
 *   the node is back.
 * - `idle` is `closed` too: the last subscriber left and the stream was let
 *   go, so whoever listens next missed what was said meanwhile.
 * - `attempt` leaves the word as it stands. `connecting` is the first
 *   attempt's alone: while the node is away the word is `closed` until the
 *   stream is open again, so a watcher that arrives between two attempts
 *   reads the gap it is in (`workspaceLoadModel.reloadOnReconnect` reads
 *   again when it ends) and the footer's dot does not change tone at every
 *   retry.
 *
 * A word or an event nobody knows changes nothing.
 * @param {"connecting" | "open" | "closed" | "lagged"} conn
 * @param {"attempt" | "opened" | "ended" | "idle"} event
 * @returns {"connecting" | "open" | "closed" | "lagged"}
 */
export function connAfter(conn, event) {
  if (event === "opened") return "open";
  if (event === "ended" || event === "idle") return "closed";
  return conn;
}

/**
 * Whether the node is there: the stream is open — a `lagged` pulse is an
 * open stream that dropped events, never a node that went away. The one rule
 * the footer's dot and the menu bar icon read.
 * @param {string} conn
 */
export function connected(conn) {
  return conn === "open" || conn === "lagged";
}

/**
 * The line a stream's end earns in the log — never louder than a warning,
 * since the node being away is no malfunction of this window. An open stream
 * that ends is said once, at `info`: it is what a node asked to stop looks
 * like from here. The first attempt that finds nobody is a warning; the ones
 * after it are `debug`, or a node that stays down would write a line every
 * fifteen seconds for as long as it does.
 * @param {boolean} wasOpen whether the stream that ended had opened
 * @param {number} attempt how many attempts have failed in a row, from 0
 * @returns {{level: "info" | "warn" | "debug", message: string}}
 */
export function endLine(wasOpen, attempt) {
  if (wasOpen) return { level: "info", message: END_WORDS.ended };
  if (!(Number(attempt) > 0)) return { level: "warn", message: END_WORDS.missed };
  return { level: "debug", message: END_WORDS.away };
}

/** What the diagnostic log is told of a stream's end — a log's lines, in the log's language. */
const END_WORDS = Object.freeze({
  ended: "the event stream ended: the node went away", // log.info
  missed: "the event stream could not be opened: the node is away", // log.warn
  away: "the node is still away", // log.debug
});

/**
 * Whether a frame is one a subscriber asked for. `key` is its own field and
 * never an alias of `scope`: an inbox frame and a conversation frame carry
 * the same ULID but answer different questions.
 * @param {{stream?: string, goal?: string, scope?: string, key?: string}} filter
 * @param {{stream: string, payload?: {goal?: string, scope?: string, key?: string}}} frame
 */
export function matches(filter, frame) {
  if (filter.stream && filter.stream !== frame.stream) return false;
  if (filter.goal) {
    if (frame.stream !== "engine") return false;
    if (frame.payload?.goal !== filter.goal) return false;
  }
  if (filter.scope) {
    if (frame.stream !== "conversation") return false;
    if (frame.payload?.scope !== filter.scope) return false;
  }
  if (filter.key) {
    if (frame.stream !== "inbox") return false;
    if (frame.payload?.key !== filter.key) return false;
  }
  return true;
}

/**
 * The frame in a message's data, or `null` when it is not one: not JSON, not
 * an object, or without the `stream` word every frame carries.
 * @param {unknown} data
 * @returns {{stream: string, payload: unknown} | null}
 */
export function parseFrame(data) {
  if (typeof data !== "string" || !data) return null;
  let parsed;
  try {
    parsed = JSON.parse(data);
  } catch {
    return null;
  }
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return null;
  if (typeof parsed.stream !== "string" || !parsed.stream) return null;
  return /** @type {{stream: string, payload: unknown}} */ (parsed);
}

/**
 * How long to wait before the next connection attempt: doubling from the
 * base, capped, and spread by up to a quarter either way so many windows do
 * not reconnect in step.
 * @param {number} attempt how many attempts have failed in a row, from 0
 * @param {number} [jitter] a draw in [0, 1); the midpoint when not given
 */
export function retryDelay(attempt, jitter = 0.5) {
  const n = Math.max(0, Math.min(Math.floor(Number(attempt) || 0), 16));
  const step = Math.min(BASE_RETRY_MS * 2 ** n, MAX_RETRY_MS);
  const draw = Number.isFinite(jitter) ? Math.min(Math.max(jitter, 0), 1) : 0.5;
  return Math.round(step * (1 - RETRY_JITTER + 2 * RETRY_JITTER * draw));
}

/**
 * Whether a frame that could not be read earns a warning now: the first one,
 * and then one per window.
 * @param {number | null} lastWarnedAt when the last warning was written, ms
 * @param {number} now
 */
export function warnsMalformed(lastWarnedAt, now) {
  return lastWarnedAt === null || now - lastWarnedAt >= MALFORMED_WARN_WINDOW_MS;
}
