/**
 * What every screen keeps of itself (`crates/desktop.md` §Per-viewer
 * state), so that leaving a screen and closing the app lose nothing: three
 * memories over one class (`keptMemoryModel.mjs`), window furniture in
 * `localStorage`.
 *
 * - `viewState` — `bisa.view.state`: each place's named values (a filter's
 *   text, the rows opened, the tab a page is on) and its scroll.
 * - `docViews` — `bisa.view.docs`: what is large and the document's own — a
 *   code editor's view, a document's mode, a design in progress.
 * - `threadPlaces` — `bisa.view.threads`: where each thread was being read.
 *
 * A **place** is a route's path (`/goals/<id>`), or a name for what is not a
 * route: `ide:<root>`, `git:<scope>`, `notes`, `draw`. `useViewState` is
 * `useState` that neither a navigation nor a restart can reach; a scroll is
 * kept quietly (`useViewScroll.ts`) — read once at mount, so writing it
 * re-renders nobody.
 *
 * The memories are written on a beat and flushed when the window hides,
 * before it reloads and on the way out (`flushMemories`, `settleMemories`).
 */

import { useCallback, useMemo, useRef, useSyncExternalStore } from "react";
import { href } from "../routeModel.mjs";
import type { Route } from "../routeModel.mjs";
import { KeptMemory, settleLeft } from "./keptMemoryModel.mjs";
import { adoptPlaceOwner, flushPlaces, forgetEveryPlace, forgetPlace, placesWrittenAt, sealPlaces } from "./placeMemoryStore";
import { webStorage } from "./storedPrefModel.mjs";
import { sameKept } from "./viewValuesModel.mjs";

const hands = {
  storage: webStorage,
  now: () => Date.now(),
  later: (fn: () => void, ms: number) => window.setTimeout(fn, ms),
  cancel: (handle: unknown) => window.clearTimeout(handle as number),
};

/** Each place's named values and its scroll. */
export const viewState = new KeptMemory({ key: "bisa.view.state", version: 1, caps: { places: 96, valueBytes: 8 * 1024, totalBytes: 256 * 1024 }, hands });
/** What is large and a document's own: an editor's view, a mode, a design in progress. */
export const docViews = new KeptMemory({ key: "bisa.view.docs", version: 1, caps: { places: 128, valueBytes: 64 * 1024, totalBytes: 1024 * 1024 }, hands });
/** Where each thread was being read. */
export const threadPlaces = new KeptMemory({ key: "bisa.view.threads", version: 1, caps: { places: 64, valueBytes: 1024, totalBytes: 32 * 1024 }, hands });

const MEMORIES = [viewState, docViews, threadPlaces] as const;

/**
 * The place a route is — its path, as the router writes it: what a routed
 * screen keeps its memory under.
 */
export function placeOf(route: Route): string {
  return href(route).slice(1);
}

/** How long the way out lets the webview's storage settle after the last write. */
const SETTLE_MS = 1000;

/**
 * `useState` that a navigation and a restart cannot reach: the value kept
 * under a place and a name, else `initial`.
 *
 * The value is read from the memory on every render — never seeded once —
 * so a screen drawn in place for another thing reads that thing's. `parse`
 * is the screen's model's: what the memory gives back that the model does
 * not know is `initial`. `store` says how a value is kept when it is not
 * JSON as it stands (a `Set` is kept as its words). Pass a stable `initial`
 * and stable functions, or none of it matters: they are held in refs.
 */
export function useViewState<T>(
  place: string,
  name: string,
  initial: T,
  parse: (raw: unknown) => T | undefined,
  store?: (value: T) => unknown,
): [T, (next: T | ((prev: T) => T)) => void] {
  const held = useRef({ initial, parse, store });
  held.current = { initial: held.current.initial, parse, store };

  const subscribe = useCallback((cb: () => void) => viewState.subscribe(place, name, cb), [place, name]);
  const snapshot = useCallback(() => viewState.read(place, name), [place, name]);
  const raw = useSyncExternalStore(subscribe, snapshot, () => undefined);
  // Parsed once per value kept: the memory hands the same value back by
  // identity until it changes, so what is drawn is stable between renders.
  const value = useMemo(() => {
    if (raw === undefined) return held.current.initial;
    const parsed = held.current.parse(raw);
    return parsed === undefined ? held.current.initial : parsed;
  }, [raw]);

  const current = useRef(value);
  current.current = value;
  const set = useCallback(
    (next: T | ((prev: T) => T)) => {
      const { initial: first, store: keepAs } = held.current;
      const resolved = typeof next === "function" ? (next as (prev: T) => T)(current.current) : next;
      const kept = keepAs ? keepAs(resolved) : resolved;
      // What a screen begins with is worth no memory.
      const beginning = keepAs ? keepAs(first) : first;
      viewState.keep(place, name, sameKept(kept, beginning) ? null : kept);
    },
    [place, name],
  );
  return [value, set];
}

/**
 * A thing is gone: what was kept of its place — and of what is under it —
 * is forgotten, and the place itself. What is large is kept under the same
 * place (a goal's design in progress), so it goes too; a document's view
 * and a thread's place are kept under keys of their own and go with their
 * root (`forgetViews`) or past their cap.
 */
export function forgetPlaceMemory(path: string): void {
  for (const memory of [viewState, docViews]) {
    memory.forget(path);
    memory.forgetUnder(`${path}/`);
  }
  forgetPlace(path);
}

/** The workspace the memories are for; another's is forgotten whole. */
export function adoptMemoryOwner(owner: string): void {
  for (const memory of MEMORIES) memory.adopt(owner);
  adoptPlaceOwner(owner);
}

/** Write what is waiting, now. */
export function flushMemories(): void {
  for (const memory of MEMORIES) memory.flush();
  flushPlaces();
}

/**
 * The way out's hand: write what is waiting, then let the webview's storage
 * settle — what is left of a second since the last write, nothing after an
 * idle moment.
 */
export async function settleMemories(): Promise<void> {
  flushMemories();
  const last = Math.max(placesWrittenAt(), ...MEMORIES.map((m) => m.writtenAt));
  const left = settleLeft(last, Date.now(), SETTLE_MS);
  if (left > 0) await new Promise<void>((done) => window.setTimeout(done, left));
}

/** Forget every place and what every screen kept — *Forget where I was*. */
export function forgetEveryMemory(): void {
  for (const memory of MEMORIES) memory.clear();
  forgetEveryPlace();
}

/**
 * What was forgotten stays forgotten until the window opens again: the
 * memories and the places take nothing more and write nothing more. Between
 * *Forget where I was* and the window's going a screen on its way out still
 * hands its last scroll over, and the window's own flush (`pagehide`) would
 * write it back.
 */
export function sealMemories(): void {
  for (const memory of MEMORIES) memory.seal();
  sealPlaces();
}

let following = false;

/**
 * Write the memories when the window is put away or is about to reload: a
 * hidden window may never come back. Called once, before the first render.
 */
export function followWindowForMemories(): void {
  if (following) return;
  following = true;
  window.addEventListener("pagehide", flushMemories);
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "hidden") flushMemories();
  });
}
