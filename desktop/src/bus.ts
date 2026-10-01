/**
 * One EventSource for the whole app, with per-view filtering.
 *
 * The previous version was a global "something changed" signal, so every
 * mounted view refetched on every frame — a tool call in one goal redrew
 * the whole app. Here a subscriber declares what it cares about (stream,
 * goal, scope, inbox key) and only hears matching frames.
 */

import { useEffect, useRef } from "react";
import { apiBaseSync, resolveApiBase, resolveApiToken, withToken } from "./api";
import { FIRST_CONN, type Conn, type StreamEvent, connAfter, endLine, matches, parseFrame, retryDelay, warnsMalformed } from "./busModel.mjs";
import { errorFields, log } from "./log";
import type { BusFrame, EngineEvent, ConversationFrame } from "./types";

/**
 * The connection as the shell sees it (`busModel.connAfter`): `connecting`
 * before the first attempt has answered, `open`, and `closed` for as long as
 * the node is away. `lagged` is a pulse, never a rest: the node said this
 * client missed events (a `system` frame), and the stores that re-read on a
 * reconnect re-read on it too — the state is `open` again in the same tick.
 */
export type ConnState = Conn;

export interface Filter {
  /** Omit for every stream. */
  stream?: "engine" | "conversation" | "inbox";
  /** Only engine frames scoped to this goal. */
  goal?: string;
  /** Only conversation frames for this conversation scope. */
  scope?: string;
  /**
   * Only inbox frames for this conversation key.
   *
   * A fourth field rather than reusing `scope`: an inbox frame and a conversation
   * frame carry the same ULID but answer different questions, and a
   * subscriber that wants "this row changed" would otherwise also be woken
   * by every message in it.
   */
  key?: string;
}

type Handler = (frame: BusFrame) => void;

interface Sub {
  filter: Filter;
  handler: Handler;
}

const subs = new Set<Sub>();
const connWatchers = new Set<(s: ConnState) => void>();

let source: EventSource | null = null;
/**
 * Every source we have ever created and not closed. `source` is the live one;
 * this is the safety net — if a bug ever leaks a second connection we can
 * still close it, instead of leaving it dispatching forever.
 */
const openSources = new Set<EventSource>();
/** In-flight `open()`, so concurrent subscribers share one connection. */
let opening: Promise<void> | null = null;
let state: ConnState = FIRST_CONN;
let attempt = 0;
let retryTimer: ReturnType<typeof setTimeout> | null = null;
/** When a frame that could not be read was last written up — one line per window, never one per frame. */
let malformedWarnedAt: number | null = null;

function setState(next: ConnState): void {
  if (state === next) return;
  state = next;
  for (const w of connWatchers) w(next);
}

/** Something happened to the stream: the connection's word is the model's. */
function happened(event: StreamEvent): void {
  setState(connAfter(state, event));
}

/**
 * The stream ended, or an attempt never opened one: the node is away. One
 * line in the log at the model's level, the word `closed`, and the next
 * attempt after the model's wait — never an error to show.
 */
function ended(wasOpen: boolean, cause?: unknown): void {
  const line = endLine(wasOpen, attempt);
  log[line.level]("bus", line.message, { attempt, ...(cause === undefined ? {} : errorFields(cause)) });
  happened("ended");
  scheduleReconnect();
}

function dispatch(frame: BusFrame): void {
  // A `system` frame is for this bus, not for a subscriber: the node dropped
  // events this client was too slow for, so everything that re-reads on a
  // reconnect re-reads now.
  if (frame.stream === "system") {
    if (frame.payload.kind === "lagged") {
      log.warn("bus", "the node dropped events for this client; the lists are read again", { dropped: frame.payload.dropped });
      setState("lagged");
      setState("open");
    }
    return;
  }
  for (const sub of subs) {
    if (matches(sub.filter, frame)) {
      try {
        sub.handler(frame);
      } catch (e) {
        log.error("bus", "a bus handler failed", errorFields(e));
      }
    }
  }
}

async function open(): Promise<void> {
  // The guard has to cover the await below, not just the synchronous entry.
  // It used to be a bare `if (source) return` — and because `source` is only
  // assigned *after* `resolveApiBase()` resolves, three subscribers mounting
  // in the same effect flush each opened their own EventSource and every
  // frame was dispatched three times.
  if (source) return;
  if (opening) return opening;
  opening = (async () => {
    happened("attempt");
    await resolveApiBase();
    await resolveApiToken();
    if (source) return; // won by another caller while we resolved
    // Nobody listens any more: the last subscriber left while the base was resolved.
    if (subs.size === 0) return;
    // An EventSource cannot send a header, so the token rides in the query.
    const es = new EventSource(withToken(`${apiBaseSync()}/events`));
    source = es;
    openSources.add(es);
    if (import.meta.env.DEV && openSources.size > 1) {
      log.warn("bus", "more than one live EventSource", { open: openSources.size });
    }
    let wasOpen = false;

    es.onopen = () => {
      wasOpen = true;
      attempt = 0;
      happened("opened");
    };
    es.onmessage = (ev) => {
      // A comment line never reaches `onmessage`, so a message that is not a
      // frame is the node's mistake — said once a minute, never per frame.
      const frame = parseFrame(ev.data);
      if (frame) {
        dispatch(frame as BusFrame);
        return;
      }
      const now = Date.now();
      if (warnsMalformed(malformedWarnedAt, now)) {
        malformedWarnedAt = now;
        log.warn("bus", "a frame could not be read", { bytes: typeof ev.data === "string" ? ev.data.length : 0 });
      }
    };
    // An `EventSource` says `error` for every way a stream ends — the node
    // asked to stop and ended it, the connection broke, nobody answered —
    // and would ask again by itself at its own pace: the bus closes it and
    // asks at the model's.
    es.onerror = () => {
      drop(es);
      ended(wasOpen);
    };
  })()
    .catch((e: unknown) => {
      // An attempt that threw before it had a stream — the shell would not
      // say the token, the address was no address — is an attempt that found
      // nobody: left unheard, the bus would never ask again.
      ended(false, e);
    })
    .finally(() => {
      opening = null;
    });
  return opening;
}

/** Close one source and forget it, whether or not it is the live one. */
function drop(es: EventSource): void {
  es.close();
  openSources.delete(es);
  if (source === es) source = null;
}

function scheduleReconnect(): void {
  if (retryTimer || subs.size === 0) return;
  // 500 ms → 15 s with a little spread, so a node that is still booting is
  // retried briskly, a node that is down does not spin, and two windows
  // dropped by one restart do not knock in step (`busModel.retryDelay`).
  const delay = retryDelay(attempt++, Math.random());
  retryTimer = setTimeout(() => {
    retryTimer = null;
    void open();
  }, delay);
}

function closeIfIdle(): void {
  if (subs.size > 0 || openSources.size === 0) return;
  for (const es of [...openSources]) drop(es);
  happened("idle");
  if (retryTimer) {
    clearTimeout(retryTimer);
    retryTimer = null;
  }
}

/** Subscribe to matching frames; returns the unsubscribe. */
export function subscribe(filter: Filter, handler: Handler): () => void {
  const sub: Sub = { filter, handler };
  subs.add(sub);
  void open();
  return () => {
    subs.delete(sub);
    closeIfIdle();
  };
}

export function watchConnection(cb: (s: ConnState) => void): () => void {
  connWatchers.add(cb);
  cb(state);
  return () => connWatchers.delete(cb);
}

/**
 * Subscribe for a component's lifetime. The handler is kept in a ref so a
 * new closure each render doesn't tear the subscription down and back up.
 */
export function useBus(filter: Filter, handler: Handler): void {
  const ref = useRef(handler);
  ref.current = handler;
  const { stream, goal, scope, key } = filter;
  useEffect(
    () => subscribe({ stream, goal, scope, key }, (f) => ref.current(f)),
    [stream, goal, scope, key],
  );
}

/** Engine frames only, already unwrapped. */
export function useEngineEvents(
  handler: (e: EngineEvent) => void,
  goal?: string,
): void {
  useBus({ stream: "engine", goal }, (f) => {
    if (f.stream === "engine") handler(f.payload);
  });
}

/** Conversation frames only, already unwrapped. */
export function useConversationEvents(handler: (f: ConversationFrame) => void, scope?: string): void {
  useBus({ stream: "conversation", scope }, (f) => {
    if (f.stream === "conversation") handler(f.payload);
  });
}
